import { EventEmitter } from "node:events";
import { PassThrough, Writable } from "node:stream";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RuntimeProcess } from "./process";

const mocks = vi.hoisted(() => ({
  spawn: vi.fn(),
  appServerPath: "/app-server/openaide-app-server",
  storageRoot: "/storage/openaide",
}));

vi.mock("node:child_process", () => ({
  spawn: mocks.spawn,
}));

vi.mock("vscode", () => ({
  workspace: {
    getConfiguration: () => ({
      get: (key: string) => {
        if (key === "appServer.path") return mocks.appServerPath;
        if (key === "storage.root") return mocks.storageRoot;
        return undefined;
      },
    }),
  },
}));

class CapturingStdin extends Writable {
  override _write(_chunk: Buffer, _encoding: BufferEncoding, callback: (error?: Error | null) => void) {
    callback();
  }
}

class FakeRuntimeChild extends EventEmitter {
  killed = false;
  stdin = new CapturingStdin();
  stdout = new PassThrough();
  stderr = new PassThrough();
  kill = vi.fn(() => {
    this.killed = true;
    this.emit("exit", null, "SIGTERM");
    return true;
  });
}

describe("RuntimeProcess App Server handoff", () => {
  beforeEach(() => {
    mocks.spawn.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("starts app server handoff mode and returns LocalHttp connection info", async () => {
    const child = new FakeRuntimeChild();
    mocks.spawn.mockReturnValue(child);
    const runtime = new RuntimeProcess(context(), logger());

    const connectionPromise = runtime.startAppServerConnection();
    child.stdout.write(JSON.stringify({
      kind: "localHttp",
      endpointUrl: "http://127.0.0.1:1234/probe",
      authToken: "token-1",
    }) + "\n");

    await expect(connectionPromise).resolves.toEqual({
      kind: "localHttp",
      endpointUrl: "http://127.0.0.1:1234/probe",
      authToken: "token-1",
    });
    expect(mocks.spawn).toHaveBeenCalledWith(
      "/app-server/openaide-app-server",
      [],
      expect.objectContaining({
        env: expect.objectContaining({
          OPENAIDE_STORAGE_ROOT: "/storage/openaide",
          OPENAIDE_APP_SERVER_PROTOCOL: "app-server-handoff",
        }),
        stdio: "pipe",
      }),
    );
  });

  it("starts shell-local app server requests in shell-control stdio mode", async () => {
    const child = new FakeRuntimeChild();
    mocks.spawn.mockReturnValue(child);
    const runtime = new RuntimeProcess(context(), logger());

    await expect(runtime.start()).resolves.toBe(child);

    expect(mocks.spawn).toHaveBeenCalledWith(
      "/app-server/openaide-app-server",
      [],
      expect.objectContaining({
        env: expect.objectContaining({
          OPENAIDE_STORAGE_ROOT: "/storage/openaide",
          OPENAIDE_APP_SERVER_PROTOCOL: "shell-control-stdio",
        }),
        stdio: "pipe",
      }),
    );
  });

  it("kills the handoff child when connection info is invalid", async () => {
    const child = new FakeRuntimeChild();
    mocks.spawn.mockReturnValue(child);
    const runtime = new RuntimeProcess(context(), logger());

    const connectionPromise = runtime.startAppServerConnection();
    child.stdout.write(JSON.stringify({ kind: "stdio" }) + "\n");

    await expect(connectionPromise).rejects.toThrow("invalid LocalHttp connection info");
    expect(child.kill).toHaveBeenCalledOnce();
  });

  it("re-handoffs after the App Server child exits", async () => {
    const first = new FakeRuntimeChild();
    const second = new FakeRuntimeChild();
    mocks.spawn.mockReturnValueOnce(first).mockReturnValueOnce(second);
    const runtime = new RuntimeProcess(context(), logger());

    const firstConnection = runtime.startAppServerConnection();
    first.stdout.write(connectionLine("1001"));
    await firstConnection;
    first.emit("exit", 0, null);

    const secondConnection = runtime.startAppServerConnection();
    second.stdout.write(connectionLine("1002"));

    await expect(secondConnection).resolves.toMatchObject({
      endpointUrl: "http://127.0.0.1:1002/probe",
    });
    expect(mocks.spawn).toHaveBeenCalledTimes(2);
  });

  it("rejects non-loopback handoff endpoints", async () => {
    const child = new FakeRuntimeChild();
    mocks.spawn.mockReturnValue(child);
    const runtime = new RuntimeProcess(context(), logger());

    const connectionPromise = runtime.startAppServerConnection();
    child.stdout.write(JSON.stringify({
      kind: "localHttp",
      endpointUrl: "https://example.com/probe",
      authToken: "token-1",
    }) + "\n");

    await expect(connectionPromise).rejects.toThrow("invalid LocalHttp connection info");
    expect(child.kill).toHaveBeenCalledOnce();
  });

  it("rejects handoff output that never sends a bounded line", async () => {
    vi.useFakeTimers();
    const child = new FakeRuntimeChild();
    mocks.spawn.mockReturnValue(child);
    const runtime = new RuntimeProcess(context(), logger());

    const connectionPromise = runtime.startAppServerConnection();
    vi.advanceTimersByTime(5_000);

    await expect(connectionPromise).rejects.toThrow("timed out");
    expect(child.kill).toHaveBeenCalledOnce();
  });

  it("rejects oversized handoff output before newline", async () => {
    const child = new FakeRuntimeChild();
    mocks.spawn.mockReturnValue(child);
    const runtime = new RuntimeProcess(context(), logger());

    const connectionPromise = runtime.startAppServerConnection();
    child.stdout.write("x".repeat(8 * 1024 + 1));

    await expect(connectionPromise).rejects.toThrow("too large");
    expect(child.kill).toHaveBeenCalledOnce();
  });
});

function context() {
  return {
    extensionUri: { fsPath: "/extension" },
    globalStorageUri: { fsPath: "/storage" },
  } as never;
}

function logger() {
  return {
    setLogFile: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  } as never;
}

function connectionLine(port: string) {
  return JSON.stringify({
    kind: "localHttp",
    endpointUrl: `http://127.0.0.1:${port}/probe`,
    authToken: `token-${port}`,
  }) + "\n";
}
