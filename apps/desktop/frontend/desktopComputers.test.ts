import { afterEach, describe, expect, it, vi } from "vitest";
import { createDesktopComputers } from "./desktopComputers";

const studio = { id: "a".repeat(64), name: "Studio", current: false };

afterEach(() => vi.restoreAllMocks());

function invoker(answers: Record<string, () => Promise<unknown>>) {
  return vi.fn((command: string, _args?: Record<string, unknown>) => answers[command]?.() ?? Promise.resolve(undefined)) as never;
}

describe("Desktop computers", () => {
  it("publishes the paired computers once the shell has read them", async () => {
    const invoke = invoker({ desktop_remote_snapshot: async () => [studio] });
    const computers = createDesktopComputers(invoke, async () => undefined);
    const changed = vi.fn();
    computers.subscribe(changed);
    expect(computers.snapshot()).toBeUndefined();
    await vi.waitFor(() => expect(changed).toHaveBeenCalled());
    expect(computers.snapshot()).toEqual({ device: "computer", computers: [studio] });
    expect(computers.snapshot()).toBe(computers.snapshot());
  });

  it("says when the list could not be read and reads it again for the next viewer", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => undefined);
    let fail = true;
    const invoke = invoker({ desktop_remote_snapshot: async () => { if (fail) throw new Error("storage"); return [studio]; } });
    const computers = createDesktopComputers(invoke, async () => undefined);
    await vi.waitFor(() => expect(computers.snapshot()?.error).toBe("OpenAIDE could not read its paired computers. Try again."));
    expect(computers.snapshot()?.computers).toEqual([]);
    fail = false;
    const unsubscribe = computers.subscribe(() => undefined);
    await vi.waitFor(() => expect(computers.snapshot()).toEqual({ device: "computer", computers: [studio] }));
    unsubscribe();
  });

  it("reloads the window only once the native side switched", async () => {
    const outcomes = ["switched", "alreadyInUse", "cancelled"];
    const invoke = invoker({
      desktop_remote_snapshot: async () => [studio],
      desktop_remote_use: async () => outcomes.shift(),
    }) as ReturnType<typeof vi.fn>;
    const reload = vi.fn(async () => undefined);
    const computers = createDesktopComputers(invoke as never, reload);
    expect(await computers.use(studio.id, "computer-switch-1")).toBe("switched");
    expect(reload).toHaveBeenCalledOnce();
    expect(await computers.use(undefined)).toBe("unchanged");
    expect(await computers.use(studio.id)).toBe("cancelled");
    expect(reload).toHaveBeenCalledOnce();
    expect(invoke.mock.calls.filter(([command]) => command === "desktop_remote_use")).toEqual([
      ["desktop_remote_use", { id: studio.id, operationId: "computer-switch-1" }],
      ["desktop_remote_use", { id: null, operationId: null }],
      ["desktop_remote_use", { id: studio.id, operationId: null }],
    ]);
  });

  it("leaves a refused switch to the caller without reloading", async () => {
    const invoke = invoker({ desktop_remote_use: async () => { throw "Studio is unreachable."; } });
    const reload = vi.fn(async () => undefined);
    const computers = createDesktopComputers(invoke, reload);
    await expect(computers.use(studio.id)).rejects.toBe("Studio is unreachable.");
    expect(reload).not.toHaveBeenCalled();
  });

  it("cancels a switch through the shell", async () => {
    const invoke = invoker({ desktop_remote_cancel: async () => true }) as ReturnType<typeof vi.fn>;
    await createDesktopComputers(invoke as never, async () => undefined).cancel!();
    expect(invoke).toHaveBeenCalledWith("desktop_remote_cancel");
  });

  it("rereads the list after forgetting or pairing unless the window moved", async () => {
    const answers = { desktop_remote_forget: "alreadyInUse", desktop_remote_pair: "switched" };
    const invoke = invoker({
      desktop_remote_snapshot: async () => [studio],
      desktop_remote_forget: async () => answers.desktop_remote_forget,
      desktop_remote_pair: async () => answers.desktop_remote_pair,
    }) as ReturnType<typeof vi.fn>;
    const reload = vi.fn(async () => undefined);
    const computers = createDesktopComputers(invoke as never, reload);
    await computers.forget(studio.id);
    await computers.pair!("OAI1 CODE");
    expect(invoke.mock.calls.filter(([command]) => command !== "desktop_remote_snapshot")).toEqual([
      ["desktop_remote_forget", { id: studio.id, operationId: null }],
      ["desktop_remote_pair", { code: "OAI1 CODE", operationId: null }],
    ]);
    expect(reload).toHaveBeenCalledOnce();
    expect(invoke.mock.calls.filter(([command]) => command === "desktop_remote_snapshot")).toHaveLength(2);
  });

  it("adds an SSH computer as a switch, rereading the list when the window stayed", async () => {
    const reload = vi.fn(async () => undefined);
    const outcomes = ["alreadyInUse", "switched"];
    const invoke = invoker({
      desktop_remote_snapshot: async () => [studio],
      desktop_ssh_add: async () => outcomes.shift(),
    });
    const computers = createDesktopComputers(invoke, reload);
    await vi.waitFor(() => expect(computers.snapshot()).toBeDefined());

    await expect(computers.ssh!.add("devbox")).resolves.toBe("unchanged");
    expect(reload).not.toHaveBeenCalled();
    await expect(computers.ssh!.add("me@lab", "Lab")).resolves.toBe("switched");

    expect(reload).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("desktop_ssh_add", { destination: "me@lab", name: "Lab", operationId: null });
    expect((invoke as unknown as { mock: { calls: unknown[][] } }).mock.calls.filter(([command]) => command === "desktop_remote_snapshot")).toHaveLength(2);
  });
});
