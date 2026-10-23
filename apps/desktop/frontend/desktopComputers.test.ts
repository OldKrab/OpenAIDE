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
    const computers = createDesktopComputers(invoke);
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
    const computers = createDesktopComputers(invoke);
    await vi.waitFor(() => expect(computers.snapshot()?.error).toBe("OpenAIDE could not read its paired computers. Try again."));
    expect(computers.snapshot()?.computers).toEqual([]);
    fail = false;
    const unsubscribe = computers.subscribe(() => undefined);
    await vi.waitFor(() => expect(computers.snapshot()).toEqual({ device: "computer", computers: [studio] }));
    unsubscribe();
  });

  it("switches, forgets and pairs through the shell, and rereads the list after forgetting", async () => {
    const invoke = invoker({ desktop_remote_snapshot: async () => [studio] }) as ReturnType<typeof vi.fn>;
    const computers = createDesktopComputers(invoke as never);
    await computers.use(studio.id);
    await computers.use();
    await computers.forget(studio.id);
    await computers.pair!("OAI1 CODE");
    expect(invoke.mock.calls.filter(([command]) => command !== "desktop_remote_snapshot")).toEqual([
      ["desktop_remote_use", { id: studio.id }],
      ["desktop_remote_use", { id: null }],
      ["desktop_remote_forget", { id: studio.id }],
      ["desktop_remote_pair", { code: "OAI1 CODE" }],
    ]);
    expect(invoke.mock.calls.filter(([command]) => command === "desktop_remote_snapshot")).toHaveLength(2);
  });
});
