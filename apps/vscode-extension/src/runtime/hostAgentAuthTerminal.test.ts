import { beforeEach, describe, expect, it, vi } from "vitest";
import { openAgentAuthTerminal, registerAgentAuthTerminalHandler } from "./hostAgentAuthTerminal";

const vscodeMocks = vi.hoisted(() => ({
  createTerminal: vi.fn(),
}));

vi.mock("vscode", () => ({
  window: {
    createTerminal: vscodeMocks.createTerminal,
  },
}));

describe("Agent authentication terminal host bridge", () => {
  beforeEach(() => vscodeMocks.createTerminal.mockReset());

  it("registers the visible authentication terminal handler", () => {
    const disposable = { dispose: vi.fn() };
    const runtime = { onHostRequest: vi.fn(() => disposable) };

    const registered = registerAgentAuthTerminalHandler(runtime as never);

    expect(runtime.onHostRequest).toHaveBeenCalledWith("agent/auth_terminal", expect.any(Function));
    expect(registered).toBe(disposable);
  });

  it("opens the exact advertised command without a shell", async () => {
    const terminal = { show: vi.fn() };
    vscodeMocks.createTerminal.mockReturnValue(terminal);

    await expect(openAgentAuthTerminal({
      command: "codex-acp",
      args: ["auth", "login"],
      env: { CODEX_HOME: "/tmp/codex" },
    })).resolves.toEqual({});

    expect(vscodeMocks.createTerminal).toHaveBeenCalledWith({
      name: "Agent sign in",
      shellPath: "codex-acp",
      shellArgs: ["auth", "login"],
      env: { CODEX_HOME: "/tmp/codex" },
    });
    expect(terminal.show).toHaveBeenCalledTimes(1);
  });

  it("rejects malformed commands before opening a terminal", async () => {
    await expect(openAgentAuthTerminal({ command: "", args: [] })).rejects.toThrow("Invalid Agent authentication terminal request");
    await expect(openAgentAuthTerminal({ command: "codex-acp", args: [1] })).rejects.toThrow("Invalid Agent authentication terminal request");
    await expect(openAgentAuthTerminal({ command: "codex-acp", env: { TOKEN: 1 } })).rejects.toThrow("Invalid Agent authentication terminal request");
    expect(vscodeMocks.createTerminal).not.toHaveBeenCalled();
  });
});
