import { EventEmitter } from "node:events";
import { PassThrough, Writable } from "node:stream";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RuntimeProcess } from "./process";

const mocks = vi.hoisted(() => ({
  spawn: vi.fn(),
  appServerPath: "/app-server/openaide-app-server",
  storageRoot: "/storage/openaide" as string | undefined,
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
  unref = vi.fn();
}

describe("RuntimeProcess App Server handoff", () => {
  let originalStorageRoot: string | undefined;

  beforeEach(() => {
    mocks.spawn.mockReset();
    mocks.storageRoot = "/storage/openaide";
    originalStorageRoot = process.env.OPENAIDE_STORAGE_ROOT;
    delete process.env.OPENAIDE_STORAGE_ROOT;
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    if (originalStorageRoot === undefined) {
      delete process.env.OPENAIDE_STORAGE_ROOT;
    } else {
      process.env.OPENAIDE_STORAGE_ROOT = originalStorageRoot;
    }
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
        detached: true,
        stdio: "pipe",
        windowsHide: true,
      }),
    );
    expect(child.unref).toHaveBeenCalledOnce();

    runtime.dispose();
    expect(child.kill).not.toHaveBeenCalled();
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

  it("uses an environment storage root when settings do not configure one", async () => {
    mocks.storageRoot = undefined;
    process.env.OPENAIDE_STORAGE_ROOT = "/storage/development";
    const child = new FakeRuntimeChild();
    mocks.spawn.mockReturnValue(child);
    const runtime = new RuntimeProcess(context(), logger());

    const connectionPromise = runtime.startAppServerConnection();
    child.stdout.write(connectionLine("1003"));
    await connectionPromise;

    expect(mocks.spawn).toHaveBeenCalledWith(
      "/app-server/openaide-app-server",
      [],
      expect.objectContaining({
        env: expect.objectContaining({
          OPENAIDE_STORAGE_ROOT: "/storage/development",
        }),
      }),
    );
    expect(runtime.describe().storage_root_kind).toBe("environment");
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

  it("does not treat a successful attach helper exit as App Server death", async () => {
    const first = new FakeRuntimeChild();
    mocks.spawn.mockReturnValue(first);
    const runtime = new RuntimeProcess(context(), logger());

    const firstConnection = runtime.startAppServerConnection();
    first.stdout.write(connectionLine("1001"));
    const connection = await firstConnection;
    first.emit("exit", 0, null);

    await expect(runtime.startAppServerConnection()).resolves.toEqual(connection);
    expect(mocks.spawn).toHaveBeenCalledOnce();
  });

  it("re-handoffs and publishes a replacement after the App Server stops responding", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("fetch", vi.fn(async () => {
      throw new Error("connection refused");
    }));
    const first = new FakeRuntimeChild();
    const second = new FakeRuntimeChild();
    mocks.spawn.mockReturnValueOnce(first).mockReturnValueOnce(second);
    const runtime = new RuntimeProcess(context(), logger());
    const replacements = vi.fn();
    runtime.onAppServerConnectionChanged(replacements);

    const firstConnection = runtime.startAppServerConnection();
    first.stdout.write(connectionLine("1001"));
    await firstConnection;
    await vi.advanceTimersByTimeAsync(10_000);
    second.stdout.write(connectionLine("1002"));
    await vi.runAllTicks();

    await vi.waitFor(() => expect(replacements).toHaveBeenCalledWith({
      kind: "localHttp",
      endpointUrl: "http://127.0.0.1:1002/probe",
      authToken: "token-1002",
    }));
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
