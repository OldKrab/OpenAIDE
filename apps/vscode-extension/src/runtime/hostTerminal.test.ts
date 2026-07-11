import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TerminalHostManager, registerTerminalHostHandlers } from "./hostTerminal";

const spawnMocks = vi.hoisted(() => ({
  spawn: vi.fn(),
}));

const fsMocks = vi.hoisted(() => ({
  realpath: vi.fn(async (path: string) => path),
}));

const fsSyncMocks = vi.hoisted(() => ({
  existsSync: vi.fn(() => false),
  readdirSync: vi.fn(() => {
    throw new Error("not found");
  }),
  statSync: vi.fn(() => ({ isDirectory: () => false })),
}));

const workspaceMocks = vi.hoisted(() => ({
  workspaceFolders: [{ uri: { fsPath: "/workspace/app" }, name: "App" }],
  terminalEnv: undefined as Record<string, string | null> | undefined,
}));

vi.mock("node:child_process", () => ({
  spawn: spawnMocks.spawn,
}));

vi.mock("node:fs/promises", () => ({
  realpath: fsMocks.realpath,
}));

vi.mock("node:fs", () => ({
  existsSync: fsSyncMocks.existsSync,
  readdirSync: fsSyncMocks.readdirSync,
  statSync: fsSyncMocks.statSync,
}));

vi.mock("vscode", () => ({
  workspace: {
    get workspaceFolders() {
      return workspaceMocks.workspaceFolders;
    },
    getConfiguration: vi.fn((section: string) => ({
      get: vi.fn((key: string) => {
        if (section === "terminal.integrated" && key === "env.linux") {
          return workspaceMocks.terminalEnv;
        }
        return undefined;
      }),
    })),
  },
}));

vi.mock("../workspace/roots", () => ({
  firstWorkspaceRoot: () => "/workspace/app",
}));

class FakeChild extends EventEmitter {
  stdout = new PassThrough();
  stderr = new PassThrough();
  killed = false;
  kill = vi.fn(() => {
    this.killed = true;
    this.emit("exit", null, "SIGTERM");
    return true;
  });
}

describe("ACP host terminal handlers", () => {
  beforeEach(() => {
    spawnMocks.spawn.mockReset();
    fsMocks.realpath.mockReset().mockImplementation(async (filePath: string) => filePath);
    fsSyncMocks.existsSync.mockReset().mockReturnValue(false);
    fsSyncMocks.readdirSync.mockReset().mockImplementation(() => {
      throw new Error("not found");
    });
    fsSyncMocks.statSync.mockReset().mockReturnValue({ isDirectory: () => false });
    workspaceMocks.workspaceFolders = [{ uri: { fsPath: "/workspace/app" }, name: "App" }];
    workspaceMocks.terminalEnv = undefined;
  });

  it("registers all terminal handlers", () => {
    const disposables = [disposable(), disposable(), disposable(), disposable(), disposable()];
    const runtime = {
      onHostRequest: vi.fn()
        .mockReturnValueOnce(disposables[0])
        .mockReturnValueOnce(disposables[1])
        .mockReturnValueOnce(disposables[2])
        .mockReturnValueOnce(disposables[3])
        .mockReturnValueOnce(disposables[4]),
    };

    const registered = registerTerminalHostHandlers(runtime as never);

    expect(runtime.onHostRequest).toHaveBeenNthCalledWith(1, "terminal/create", expect.any(Function));
    expect(runtime.onHostRequest).toHaveBeenNthCalledWith(2, "terminal/output", expect.any(Function));
    expect(runtime.onHostRequest).toHaveBeenNthCalledWith(3, "terminal/wait_for_exit", expect.any(Function));
    expect(runtime.onHostRequest).toHaveBeenNthCalledWith(4, "terminal/kill", expect.any(Function));
    expect(runtime.onHostRequest).toHaveBeenNthCalledWith(5, "terminal/release", expect.any(Function));

    registered.dispose();
    expect(disposables.every((item) => item.dispose.mock.calls.length === 1)).toBe(true);
  });

  it("creates a workspace-scoped process and captures bounded output", async () => {
    const child = new FakeChild();
    spawnMocks.spawn.mockReturnValue(child);
    const manager = new TerminalHostManager();

    const created = await manager.create({
      sessionId: "session_1",
      command: "node",
      args: ["script.js"],
      cwd: "/workspace/app",
      outputByteLimit: 5,
    });
    child.stdout.write("hello ");
    child.stderr.write("world");

    expect(spawnMocks.spawn).toHaveBeenCalledWith("node", ["script.js"], expect.objectContaining({
      cwd: "/workspace/app",
      shell: false,
    }));
    expect(manager.output(terminalRef(created))).toEqual({
      output: "world",
      truncated: true,
    });
  });

  it("applies terminal env settings and repairs Codex bundled tool PATH entries", async () => {
    const originalPath = process.env.PATH;
    const originalHome = process.env.HOME;
    process.env.PATH = ["/usr/bin", "/codex/vendor/x86_64-unknown-linux-musl/path"].join(":");
    process.env.HOME = "/home/test";
    workspaceMocks.terminalEnv = {
      PATH: "${env:PATH}:/configured/bin",
    };
    fsSyncMocks.existsSync.mockImplementation((filePath: string) => filePath === "/codex/vendor/x86_64-unknown-linux-musl/codex-path");
    fsSyncMocks.statSync.mockImplementation(() => ({ isDirectory: () => true }));

    try {
      const child = new FakeChild();
      spawnMocks.spawn.mockReturnValue(child);
      const manager = new TerminalHostManager();

      await manager.create({ sessionId: "session_1", command: "zsh", args: ["-lc", "rg test"], cwd: "/workspace/app" });

      const options = spawnMocks.spawn.mock.calls[0][2] as { env: NodeJS.ProcessEnv };
      expect(options.env.PATH?.split(":")).toEqual([
        "/usr/bin",
        "/codex/vendor/x86_64-unknown-linux-musl/path",
        "/codex/vendor/x86_64-unknown-linux-musl/codex-path",
        "/configured/bin",
      ]);
    } finally {
      process.env.PATH = originalPath;
      process.env.HOME = originalHome;
    }
  });

  it("discovers Codex bundled tool paths from an nvm install", async () => {
    const originalPath = process.env.PATH;
    const originalHome = process.env.HOME;
    process.env.PATH = "/usr/bin";
    process.env.HOME = "/home/test";
    fsSyncMocks.existsSync.mockImplementation((filePath: string) => {
      return filePath === "/home/test/.nvm/versions/node/v25/lib/node_modules/@openai/codex/node_modules/@openai/codex-linux-x64/vendor/x86/codex-path";
    });
    fsSyncMocks.statSync.mockImplementation(() => ({ isDirectory: () => true }));
    fsSyncMocks.readdirSync.mockImplementation((parent: string) => {
      const children: Record<string, string[]> = {
        "/home/test/.nvm/versions/node": ["v25"],
        "/home/test/.nvm/versions/node/v25/lib/node_modules/@openai/codex/node_modules": ["@openai"],
        "/home/test/.nvm/versions/node/v25/lib/node_modules/@openai/codex/node_modules/@openai": ["codex-linux-x64"],
        "/home/test/.nvm/versions/node/v25/lib/node_modules/@openai/codex/node_modules/@openai/codex-linux-x64/vendor": ["x86"],
      };
      const names = children[parent];
      if (!names) throw new Error("not found");
      return names.map((name) => ({ name, isDirectory: () => true }));
    });

    try {
      const child = new FakeChild();
      spawnMocks.spawn.mockReturnValue(child);
      const manager = new TerminalHostManager();

      await manager.create({ sessionId: "session_1", command: "zsh", args: ["-lc", "rg test"], cwd: "/workspace/app" });

      const options = spawnMocks.spawn.mock.calls[0][2] as { env: NodeJS.ProcessEnv };
      expect(options.env.PATH?.split(":")).toEqual([
        "/usr/bin",
        "/home/test/.nvm/versions/node/v25/lib/node_modules/@openai/codex/node_modules/@openai/codex-linux-x64/vendor/x86/codex-path",
      ]);
    } finally {
      process.env.PATH = originalPath;
      process.env.HOME = originalHome;
    }
  });

  it("waits for exit and returns exit status", async () => {
    const child = new FakeChild();
    spawnMocks.spawn.mockReturnValue(child);
    const manager = new TerminalHostManager();

    const created = await manager.create({ sessionId: "session_1", command: "npm", cwd: "/workspace/app" });
    const wait = manager.waitForExit(terminalRef(created));
    child.emit("exit", 0, null);

    await expect(wait).resolves.toEqual({ exitCode: 0, signal: null });
    expect(manager.output(terminalRef(created))).toMatchObject({
      exitStatus: { exitCode: 0, signal: null },
    });
  });

  it("rejects terminal cwd outside workspace after realpath", async () => {
    fsMocks.realpath.mockImplementation(async (filePath: string) => {
      if (filePath === "/workspace/app") return "/workspace/app";
      if (filePath === "/workspace/app/link") return "/tmp/outside";
      return filePath;
    });
    const manager = new TerminalHostManager();

    await expect(manager.create({ sessionId: "session_1", command: "pwd", cwd: "/workspace/app/link" })).rejects.toThrow(
      "outside the current workspace",
    );
    expect(spawnMocks.spawn).not.toHaveBeenCalled();
  });

  it("kills and releases terminals", async () => {
    const child = new FakeChild();
    spawnMocks.spawn.mockReturnValue(child);
    const manager = new TerminalHostManager();

    const created = await manager.create({ sessionId: "session_1", command: "sleep", args: ["10"], cwd: "/workspace/app" });
    manager.kill(terminalRef(created));
    expect(child.kill).toHaveBeenCalledTimes(1);

    await manager.release(terminalRef(created));
    expect(() => manager.output(terminalRef(created))).toThrow("terminal not found");
  });

  it("does not allow another session to control a terminal id", async () => {
    const child = new FakeChild();
    spawnMocks.spawn.mockReturnValue(child);
    const manager = new TerminalHostManager();

    const created = await manager.create({ sessionId: "session_1", command: "npm", cwd: "/workspace/app" });

    expect(() => manager.output({ sessionId: "session_2", terminalId: created.terminalId })).toThrow("terminal not found");
    expect(() => manager.kill({ sessionId: "session_2", terminalId: created.terminalId })).toThrow("terminal not found");
    expect(child.kill).not.toHaveBeenCalled();
  });

  it("escalates kill while keeping the terminal valid until exit", async () => {
    vi.useFakeTimers();
    try {
      const child = new FakeChild();
      child.kill.mockImplementation(() => {
        child.killed = true;
        return true;
      });
      spawnMocks.spawn.mockReturnValue(child);
      const manager = new TerminalHostManager();
      const created = await manager.create({ sessionId: "session_1", command: "sleep", cwd: "/workspace/app" });
      const wait = manager.waitForExit(terminalRef(created)) as Promise<unknown>;

      manager.kill(terminalRef(created));
      expect(() => manager.output(terminalRef(created))).not.toThrow();
      expect(child.kill).toHaveBeenCalledTimes(1);

      vi.advanceTimersByTime(2_000);
      expect(child.kill).toHaveBeenCalledTimes(2);
      expect(child.kill).toHaveBeenLastCalledWith("SIGKILL");
      expect(() => manager.output(terminalRef(created))).not.toThrow();

      child.emit("exit", null, "SIGKILL");
      await expect(wait).resolves.toEqual({ exitCode: null, signal: "SIGKILL" });
      expect(manager.output(terminalRef(created))).toMatchObject({
        exitStatus: { exitCode: null, signal: "SIGKILL" },
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it("invalidates released terminals immediately while retaining process control", async () => {
    vi.useFakeTimers();
    try {
      const child = new FakeChild();
      child.kill.mockImplementation(() => {
        child.killed = true;
        return true;
      });
      spawnMocks.spawn.mockReturnValue(child);
      const manager = new TerminalHostManager();
      const created = await manager.create({ sessionId: "session_1", command: "sleep", cwd: "/workspace/app" });

      manager.release(terminalRef(created));
      expect(() => manager.output(terminalRef(created))).toThrow("terminal not found");
      expect(child.kill).toHaveBeenCalledTimes(1);

      vi.advanceTimersByTime(2_000);
      expect(child.kill).toHaveBeenCalledTimes(2);
      expect(child.kill).toHaveBeenLastCalledWith("SIGKILL");

      child.emit("exit", null, "SIGTERM");
      expect(() => manager.output(terminalRef(created))).toThrow("terminal not found");
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps stdout and stderr UTF-8 decoder state separate", async () => {
    const child = new FakeChild();
    spawnMocks.spawn.mockReturnValue(child);
    const manager = new TerminalHostManager();
    const created = await manager.create({ sessionId: "session_1", command: "node", cwd: "/workspace/app" });
    const euro = Buffer.from("€", "utf8");

    child.stdout.write(euro.subarray(0, 1));
    child.stderr.write("x");
    child.stdout.write(euro.subarray(1));

    expect(manager.output(terminalRef(created))).toMatchObject({ output: "x€" });
  });

  it("truncates output without splitting UTF-8 characters", async () => {
    const child = new FakeChild();
    spawnMocks.spawn.mockReturnValue(child);
    const manager = new TerminalHostManager();
    const created = await manager.create({
      sessionId: "session_1",
      command: "node",
      cwd: "/workspace/app",
      outputByteLimit: 3,
    });

    child.stdout.write(Buffer.from("a€", "utf8"));

    expect(manager.output(terminalRef(created))).toMatchObject({
      output: "€",
      truncated: true,
    });
  });
});

function disposable() {
  return { dispose: vi.fn() };
}

function terminalRef(created: { terminalId: string }) {
  return { sessionId: "session_1", terminalId: created.terminalId };
}
