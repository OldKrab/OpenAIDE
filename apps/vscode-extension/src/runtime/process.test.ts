import { EventEmitter } from "node:events";
import { Writable } from "node:stream";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { requestRuntimeShutdown } from "./process";

vi.mock("vscode", () => ({
  workspace: {
    getConfiguration: () => ({ get: () => undefined }),
  },
}));

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
  kill = vi.fn(() => {
    this.killed = true;
    this.emit("exit", null, "SIGTERM");
    return true;
  });
}

describe("runtime process shutdown", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("requests runtime.shutdown before killing the child after a grace timeout", () => {
    const child = new FakeRuntimeChild();

    requestRuntimeShutdown(child as never, 250);

    const written = child.stdin.chunks.join("");
    expect(written).toContain('"method":"runtime.shutdown"');
    expect(child.kill).not.toHaveBeenCalled();

    vi.advanceTimersByTime(250);

    expect(child.kill).toHaveBeenCalledTimes(1);
  });

  it("does not kill when the runtime exits during the grace timeout", () => {
    const child = new FakeRuntimeChild();

    requestRuntimeShutdown(child as never, 250);
    child.emit("exit", 0, null);
    vi.advanceTimersByTime(250);

    expect(child.kill).not.toHaveBeenCalled();
  });
});
