import { EventEmitter } from "node:events";
import { PassThrough, Writable } from "node:stream";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RuntimeClient } from "./rpcClient";

vi.mock("vscode", () => ({}));

class CapturingStdin extends Writable {
  chunks: string[] = [];

  override _write(chunk: Buffer, _encoding: BufferEncoding, callback: (error?: Error | null) => void) {
    this.chunks.push(chunk.toString("utf8"));
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

class FakeRuntimeProcess {
  child = new FakeRuntimeChild();
  start = vi.fn(async () => this.child);

  onExit(listener: (event: { code: number | null; signal: NodeJS.Signals | null }) => void) {
    this.child.once("exit", (code: number | null, signal: NodeJS.Signals | null) => listener({ code, signal }));
    return { dispose: vi.fn() };
  }
}

describe("runtime client host requests", () => {
  let process: FakeRuntimeProcess;
  let client: RuntimeClient;

  beforeEach(async () => {
    process = new FakeRuntimeProcess();
    client = new RuntimeClient(process as never, logger() as never);
    const health = client.health();
    await flush();

    const request = writtenMessages(process.child.stdin)[0];
    process.child.stdout.write(`${JSON.stringify({
      jsonrpc: "2.0",
      id: request.id,
      result: { status: "ready", version: "test", methods: [] },
    })}\n`);
    await health;
  });

  it("responds to runtime-initiated host requests with handler results", async () => {
    client.onHostRequest("host/echo", async (params) => ({ seen: params }));

    process.child.stdout.write(`${JSON.stringify({
      jsonrpc: "2.0",
      id: "host_1",
      method: "host/echo",
      params: { value: 42 },
    })}\n`);
    await flush();

    expect(writtenMessages(process.child.stdin).at(-1)).toEqual({
      jsonrpc: "2.0",
      id: "host_1",
      result: { seen: { value: 42 } },
    });
  });

  it("returns method-not-found for unregistered host request methods", async () => {
    process.child.stdout.write(`${JSON.stringify({
      jsonrpc: "2.0",
      id: "host_2",
      method: "host/missing",
      params: {},
    })}\n`);
    await flush();

    expect(writtenMessages(process.child.stdin).at(-1)).toEqual({
      jsonrpc: "2.0",
      id: "host_2",
      error: {
        code: -32601,
        message: "Host method not found: host/missing",
      },
    });
  });

  it("attaches one runtime line reader for concurrent first requests", async () => {
    process = new FakeRuntimeProcess();
    client = new RuntimeClient(process as never, logger() as never);

    const first = client.health();
    const second = client.health();
    await flush();

    const [firstRequest, secondRequest] = writtenMessages(process.child.stdin);
    process.child.stdout.write(`${JSON.stringify({
      jsonrpc: "2.0",
      id: firstRequest.id,
      result: { status: "ready", version: "test", methods: [] },
    })}\n`);
    process.child.stdout.write(`${JSON.stringify({
      jsonrpc: "2.0",
      id: secondRequest.id,
      result: { status: "ready", version: "test", methods: [] },
    })}\n`);
    await Promise.all([first, second]);

    client.onHostRequest("host/echo", () => ({ ok: true }));
    process.child.stdout.write(`${JSON.stringify({
      jsonrpc: "2.0",
      id: "host_race",
      method: "host/echo",
      params: {},
    })}\n`);
    await flush();

    const hostResponses = writtenMessages(process.child.stdin).filter((message) => message.id === "host_race");
    expect(hostResponses).toEqual([
      {
        jsonrpc: "2.0",
        id: "host_race",
        result: { ok: true },
      },
    ]);
  });
});

function writtenMessages(stdin: CapturingStdin) {
  return stdin.chunks
    .join("")
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function flush() {
  return new Promise<void>((resolve) => setImmediate(resolve));
}

function logger() {
  return {
    warn: vi.fn(),
    info: vi.fn(),
    error: vi.fn(),
  };
}
