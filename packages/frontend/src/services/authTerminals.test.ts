import { afterEach, describe, expect, it, vi } from "vitest";
import { authTerminalForAgent, closeAuthTerminals, handleAuthTerminal } from "./authTerminals";

const frame = { agentId: "fixture", terminalId: "login-1", output: btoa("Sign in: "), exited: false };
afterEach(() => { closeAuthTerminals(); vi.useRealTimers(); });

describe("client-scoped auth terminals", () => {
  it("renders exact bytes and sends each input batch once", async () => {
    vi.useFakeTimers();
    const response = handleAuthTerminal(frame, new AbortController().signal);
    const session = authTerminalForAgent("fixture")!;
    const output = vi.fn();
    session.attach(output);
    expect(new TextDecoder().decode(output.mock.calls[0][0])).toBe("Sign in: ");
    session.send("one-time-code\r");
    await vi.advanceTimersByTimeAsync(100);
    expect(atob((await response).input)).toBe("one-time-code\r");
    const next = handleAuthTerminal({ ...frame, output: "" }, new AbortController().signal);
    await vi.advanceTimersByTimeAsync(100);
    expect((await next).input).toBe("");
    const remounted = vi.fn();
    session.attach(remounted);
    expect(new TextDecoder().decode(remounted.mock.calls[0][0])).toBe("Sign in: ");
  });

  it("cancels pending input and removes the terminal on disconnect", async () => {
    const controller = new AbortController();
    const response = handleAuthTerminal(frame, controller.signal);
    controller.abort();
    expect((await response).cancel).toBe(true);
    expect(authTerminalForAgent("fixture")).toBeUndefined();
  });

  it("removes exited sessions without declaring success from terminal text", async () => {
    const response = await handleAuthTerminal({ ...frame, exited: true }, new AbortController().signal);
    expect(response.cancel).toBe(false);
    expect(authTerminalForAgent("fixture")).toBeUndefined();
  });
});
