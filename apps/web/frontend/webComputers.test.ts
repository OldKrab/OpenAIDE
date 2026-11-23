import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ComputersCapability } from "../../../packages/frontend/src/services/computers";
import { createWebComputers } from "./webComputers";

const DESK = "d".repeat(64);
const LAPTOP = "a".repeat(64);
const CLIENT_KEY = "openaide.clientInstanceId";

type Route = (body: any, signal?: AbortSignal) => Response | Promise<Response>;

beforeEach(() => {
  const items = new Map<string, string>([[CLIENT_KEY, "client-before"]]);
  vi.stubGlobal("sessionStorage", {
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => void items.set(key, value),
    removeItem: (key: string) => void items.delete(key),
  });
});
afterEach(() => vi.unstubAllGlobals());

/** The capability reads the list at once, so `list` answers that first read. */
function setUp(pinned?: string, paired = [{ id: DESK, name: "Desk" }, { id: LAPTOP, name: "Laptop" }], list?: Route) {
  const calls: { action: string; body: any }[] = [];
  const routes: Record<string, Route> = { list: list ?? (() => answer(200, { computers: paired })) };
  const logs: { level: string; event: string; fields: Record<string, unknown> }[] = [];
  // Each reload records the client instance id the next page would find.
  const reloads: (string | null)[] = [];
  const capability = createWebComputers({
    pinned,
    fetch: async (input, init) => {
      const action = String(input).split("/").pop()!;
      const body = init?.body ? JSON.parse(String(init.body)) : undefined;
      calls.push({ action, body });
      return routes[action](body, init?.signal ?? undefined);
    },
    reload: () => reloads.push(sessionStorage.getItem(CLIENT_KEY)),
    log: (level, event, fields) => logs.push({ level, event, fields }),
  });
  return { capability, calls, routes, logs, reloads };
}

function answer(status: number, body: unknown) {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

function refusal(reason: string, message: string) {
  return answer(422, { error: { reason, message } });
}

/** Settles on the next list the capability publishes. */
function published(capability: ComputersCapability) {
  return new Promise<void>((resolve) => {
    const off = capability.subscribe(() => {
      off();
      resolve();
    });
  });
}

/** A route that answers only once the test opens it, failing as a fetch does when aborted first. */
function held() {
  let open!: (response: Response) => void;
  const route: Route = (_body, signal) => new Promise<Response>((resolve, reject) => {
    open = resolve;
    signal?.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
  });
  return { route, open: (response: Response) => open(response) };
}

describe("Web App computers", () => {
  it("lists the paired computers with the one this page opened on in use", async () => {
    const { capability } = setUp(LAPTOP);
    await published(capability);
    expect(capability.snapshot()).toEqual({
      device: "computer",
      computers: [
        { id: DESK, name: "Desk", current: false },
        { id: LAPTOP, name: "Laptop", current: true },
      ],
    });
  });

  it("keeps the computer this page opened on in use after another tab forgot it", async () => {
    const { capability } = setUp(LAPTOP, [{ id: DESK, name: "Desk" }]);
    await published(capability);
    expect(capability.snapshot()?.computers).toEqual([
      { id: DESK, name: "Desk", current: false },
      { id: LAPTOP, name: "", current: true },
    ]);
  });

  it("says when the list could not be read and reads it again for a new watcher", async () => {
    const { capability, routes } = setUp(LAPTOP, [], () => answer(503, { error: { reason: "gateway_unavailable", message: "Unavailable" } }));
    await published(capability);
    expect(capability.snapshot()).toEqual({
      device: "computer",
      computers: [{ id: LAPTOP, name: "", current: true }],
      error: "OpenAIDE could not read its paired computers. Try again.",
    });

    routes.list = () => answer(200, { computers: [{ id: LAPTOP, name: "Laptop" }] });
    await published(capability);
    expect(capability.snapshot()).toEqual({ device: "computer", computers: [{ id: LAPTOP, name: "Laptop", current: true }] });
  });

  it("reloads onto a computer as a new client only once the server reached it", async () => {
    const { capability, calls, logs, reloads, routes } = setUp();
    await published(capability);
    const use = held();
    routes.use = use.route;

    const switching = capability.use(DESK, "computer-switch-1");
    // The request is sent before `use` first yields.
    expect(calls.at(-1)).toEqual({ action: "use", body: { computer: DESK, operationId: "computer-switch-1" } });
    expect(reloads).toEqual([]);

    use.open(answer(200, {}));
    expect(await switching).toBe("switched");
    expect(reloads).toEqual([null]);
    expect(logs.filter((log) => log.event === "web_computers_call_completed" && log.fields.action === "use"))
      .toEqual([expect.objectContaining({ level: "info", fields: expect.objectContaining({ operation_id: "computer-switch-1", outcome: "ok", status: 200 }) })]);
  });

  it("changes nothing for the computer already in use", async () => {
    const onDesk = setUp(DESK);
    await published(onDesk.capability);
    expect(await onDesk.capability.use(DESK)).toBe("unchanged");
    const here = setUp();
    await published(here.capability);
    expect(await here.capability.use(undefined)).toBe("unchanged");
    expect([...onDesk.calls, ...here.calls].map((call) => call.action)).toEqual(["list", "list"]);
  });

  it("moves back to the Web App's own computer", async () => {
    const { capability, calls, reloads, routes } = setUp(DESK);
    await published(capability);
    routes.use = () => answer(200, {});
    expect(await capability.use(undefined, "computer-switch-2")).toBe("switched");
    expect(calls.at(-1)).toEqual({ action: "use", body: { computer: null, operationId: "computer-switch-2" } });
    expect(reloads).toEqual([null]);
  });

  it("stays on the page with the server's sentence when the computer cannot be reached", async () => {
    const { capability, logs, reloads, routes } = setUp();
    await published(capability);
    routes.use = () => refusal("unreachable", "Desk could not be reached.");

    await expect(capability.use(DESK, "computer-switch-3")).rejects.toThrow("Desk could not be reached.");
    expect(reloads).toEqual([]);
    expect(logs.at(-1)).toEqual({
      level: "warn",
      event: "web_computers_call_completed",
      fields: expect.objectContaining({ action: "use", operation_id: "computer-switch-3", outcome: "refused", status: 422, reason: "unreachable" }),
    });
    // The sentence is for the user; diagnostics keep only its reason.
    expect(JSON.stringify(logs)).not.toContain("could not be reached");
  });

  it("gives its own sentence when the Web App's server does not answer", async () => {
    const { capability, routes } = setUp();
    await published(capability);
    routes.use = () => Promise.reject(new TypeError("Failed to fetch"));
    await expect(capability.use(DESK)).rejects.toThrow("The Web App's server did not answer. Try again.");
  });

  it("cancels a switch the server has not answered", async () => {
    const { capability, calls, reloads, routes } = setUp();
    await published(capability);
    const use = held();
    routes.use = use.route;

    const switching = capability.use(DESK, "computer-switch-4");
    expect(calls.at(-1)?.action).toBe("use");
    await capability.cancel!();
    expect(await switching).toBe("cancelled");
    expect(reloads).toEqual([]);
  });

  it("finishes a switch whose answer arrived before the cancel, since the choice is already kept", async () => {
    const { capability, calls, reloads, routes } = setUp();
    await published(capability);
    let headersSent!: () => void;
    const answered = new Promise<void>((resolve) => { headersSent = resolve; });
    // The status arrived; the body is still streaming when the cancel aborts it.
    routes.use = (_body, signal) => {
      const body = new ReadableStream({
        start(controller) {
          signal?.addEventListener("abort", () => controller.error(new DOMException("aborted", "AbortError")));
        },
      });
      headersSent();
      return new Response(body, { status: 200 });
    };

    const switching = capability.use(DESK);
    await answered;
    await capability.cancel!();
    expect(await switching).toBe("switched");
    expect(reloads).toEqual([null]);
  });

  it("moves to the Web App's own computer when the computer in use is forgotten", async () => {
    const { capability, calls, reloads, routes } = setUp(DESK);
    await published(capability);
    routes.forget = () => answer(200, {});
    routes.use = () => answer(200, {});

    await capability.forget(DESK);
    expect(calls.slice(1)).toEqual([
      { action: "forget", body: { computer: DESK } },
      { action: "use", body: { computer: null } },
    ]);
    expect(reloads).toEqual([null]);
  });

  it("reads the list again after forgetting another computer", async () => {
    const { capability, calls, reloads, routes } = setUp(DESK);
    await published(capability);
    routes.forget = () => {
      routes.list = () => answer(200, { computers: [{ id: DESK, name: "Desk" }] });
      return answer(200, {});
    };

    await capability.forget(LAPTOP);
    expect(calls.slice(1).map((call) => call.action)).toEqual(["forget", "list"]);
    expect(capability.snapshot()?.computers).toEqual([{ id: DESK, name: "Desk", current: true }]);
    expect(reloads).toEqual([]);
  });

  it("uses a computer once paired, and keeps a paired one it could not reach listed", async () => {
    const { capability, calls, logs, reloads, routes } = setUp(undefined, []);
    await published(capability);
    routes.pair = () => {
      routes.list = () => answer(200, { computers: [{ id: DESK, name: "Desk" }] });
      return refusal("unreachable", "Desk could not be reached.");
    };

    await expect(capability.pair!("secret-code", { key: "server-key", name: "Server" })).rejects.toThrow("Desk could not be reached.");
    expect(calls.slice(1)).toEqual([
      { action: "pair", body: { code: "secret-code", inUse: { key: "server-key", name: "Server" } } },
      { action: "list", body: undefined },
    ]);
    expect(capability.snapshot()?.computers).toEqual([{ id: DESK, name: "Desk", current: false }]);
    expect(reloads).toEqual([]);

    routes.pair = () => answer(200, { computer: { id: LAPTOP, name: "Laptop" } });
    await capability.pair!("secret-code");
    expect(calls.at(-1)).toEqual({ action: "pair", body: { code: "secret-code", inUse: null } });
    expect(reloads).toEqual([null]);
    expect(JSON.stringify(logs)).not.toContain("secret-code");
  });
});
