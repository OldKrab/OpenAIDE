import { describe, expect, it, vi } from "vitest";
import type { RpcMessage } from "./rpcPeer";
import {
  createReliableHttpMessageChannel,
  type ReliableHttpFetch,
} from "./reliableHttpChannel";

describe("ReliableHttpMessageChannel", () => {
  it("retries the identical sequenced upload and receives the response through polling", async () => {
    const uploadBodies: string[] = [];
    let uploadAttempt = 0;
    let pollAttempt = 0;
    const fetch = vi.fn<ReliableHttpFetch>(async (_input, init) => {
      if (init.method === "POST" && init.body?.includes('"transport":"open"')) {
        return response(200, JSON.stringify({
          transportVersion: 1,
          sessionId: "session-1",
          serverId: "server-1",
        }));
      }
      if (init.method === "POST") {
        uploadBodies.push(init.body ?? "");
        uploadAttempt += 1;
        if (uploadAttempt === 1) throw new Error("acknowledgement lost");
        return response(204, "");
      }
      pollAttempt += 1;
      if (pollAttempt === 1) {
        return response(200, JSON.stringify({
          frames: [{
            sequence: 1,
            message: { jsonrpc: "2.0", id: "rpc-1", result: 42 },
          }],
        }));
      }
      return new Promise(() => undefined);
    });
    const channel = createReliableHttpMessageChannel({
      endpointUrl: "http://127.0.0.1:4321",
      authToken: "token-1",
      connectionId: "client-1",
      fetch,
      retryDelayMs: 0,
    });
    const received: RpcMessage[] = [];
    channel.subscribe((message) => received.push(message));

    channel.send({
      jsonrpc: "2.0",
      id: "rpc-1",
      method: "math/add",
      params: { left: 20, right: 22 },
    });

    await vi.waitFor(() => expect(received).toEqual([
      { jsonrpc: "2.0", id: "rpc-1", result: 42 },
    ]));
    await vi.waitFor(() => expect(uploadBodies).toHaveLength(2));
    expect(uploadBodies[1]).toBe(uploadBodies[0]);
    channel.close();
  });

  it("falls back to in-memory chunks when an intermediary rejects a large upload", async () => {
    const chunkBodies: string[] = [];
    let rejectedBody = "";
    let receivePolls = 0;
    const fetch = vi.fn<ReliableHttpFetch>(async (_input, init) => {
      if (init.method === "GET") {
        receivePolls += 1;
        return new Promise(() => undefined);
      }
      const body = init.body ?? "";
      if (body.includes('"transport":"open"')) {
        return response(200, JSON.stringify({
          transportVersion: 1,
          sessionId: "session-1",
          serverId: "server-1",
        }));
      }
      const envelope = JSON.parse(body) as { transport?: string; offset?: number; totalSize?: number; data?: string };
      if (envelope.transport !== "chunk") {
        rejectedBody = body;
        return response(403, "<!doctype html><html><body>request too large</body></html>");
      }
      chunkBodies.push(body);
      const received = (envelope.offset ?? 0) + decodeBase64(envelope.data ?? "").byteLength;
      return response(received === envelope.totalSize ? 204 : 202, "");
    });
    const channel = createReliableHttpMessageChannel({
      endpointUrl: "http://127.0.0.1:4321",
      connectionId: "client-1",
      fetch,
      retryDelayMs: 0,
      deferReceiveUntilFirstUpload: true,
    });
    const errors: unknown[] = [];
    channel.subscribeErrors?.((error) => errors.push(error));

    channel.send({
      jsonrpc: "2.0",
      id: "rpc-large-image",
      method: "task/send",
      params: { image: "x".repeat(1_300_000) },
    });

    await vi.waitFor(() => expect(chunkBodies.length).toBeGreaterThanOrEqual(3));
    await vi.waitFor(() => expect(receivePolls).toBe(1));
    const chunks = chunkBodies.map((body) => JSON.parse(body) as {
      data: string;
      offset: number;
      totalSize: number;
    });
    expect(chunkBodies.every((body) => new TextEncoder().encode(body).byteLength < 1_000_000)).toBe(true);
    const reconstructed = new Uint8Array(chunks.reduce((size, chunk) => size + decodeBase64(chunk.data).byteLength, 0));
    let reconstructedOffset = 0;
    for (const chunk of chunks) {
      const bytes = decodeBase64(chunk.data);
      reconstructed.set(bytes, reconstructedOffset);
      reconstructedOffset += bytes.byteLength;
    }
    expect(new TextDecoder().decode(reconstructed)).toBe(rejectedBody);
    expect(chunks.map((chunk) => chunk.offset)).toEqual([0, 512 * 1024, 1024 * 1024]);
    expect(errors).toEqual([]);
    channel.close();
  });

  it("reports authoritative session loss instead of retrying it forever", async () => {
    const fetch = vi.fn<ReliableHttpFetch>(async (_input, init) => {
      if (init.method === "POST") {
        return response(200, JSON.stringify({
          transportVersion: 1,
          sessionId: "session-1",
          serverId: "server-1",
        }));
      }
      return response(410, "");
    });
    const channel = createReliableHttpMessageChannel({
      endpointUrl: "http://127.0.0.1:4321",
      connectionId: "client-1",
      fetch,
      retryDelayMs: 0,
    });
    const errors: unknown[] = [];
    channel.subscribeErrors?.((error) => errors.push(error));

    await vi.waitFor(() => expect(errors).toHaveLength(1));
    expect(String(errors[0])).toContain("HTTP 410");
    expect(fetch.mock.calls.filter(([, init]) => init.method === "GET")).toHaveLength(1);
    channel.close();
  });

  it("restarts a suspended receive poll and replays from the last delivered sequence", async () => {
    let wake: (() => void) | undefined;
    let pollAttempt = 0;
    const pollAfter: string[] = [];
    const fetch = vi.fn<ReliableHttpFetch>(async (_input, init) => {
      if (init.method === "POST") {
        return response(200, JSON.stringify({
          transportVersion: 1,
          sessionId: "session-1",
          serverId: "server-1",
        }));
      }
      pollAttempt += 1;
      pollAfter.push(init.headers["X-OpenAIDE-After"] ?? "");
      if (pollAttempt === 1) {
        return new Promise((_resolve, reject) => {
          init.signal?.addEventListener("abort", () => reject(abortError()), { once: true });
        });
      }
      if (pollAttempt === 2) {
        return response(200, JSON.stringify({
          frames: [{
            sequence: 1,
            message: { jsonrpc: "2.0", id: "rpc-1", result: 42 },
          }],
        }));
      }
      return new Promise(() => undefined);
    });
    const channel = createReliableHttpMessageChannel({
      endpointUrl: "http://127.0.0.1:4321",
      connectionId: "client-1",
      fetch,
      retryDelayMs: 0,
      subscribeToWake(listener) {
        wake = listener;
        return () => {
          wake = undefined;
        };
      },
    });
    const received: RpcMessage[] = [];
    channel.subscribe((message) => received.push(message));
    await vi.waitFor(() => expect(pollAttempt).toBe(1));

    wake?.();

    await vi.waitFor(() => expect(received).toEqual([
      { jsonrpc: "2.0", id: "rpc-1", result: 42 },
    ]));
    expect(pollAfter.slice(0, 2)).toEqual(["0", "0"]);
    channel.close();
  });

  it("restarts a receive poll that outlives the server hold deadline", async () => {
    let pollAttempt = 0;
    const fetch = vi.fn<ReliableHttpFetch>(async (_input, init) => {
      if (init.method === "POST") {
        return response(200, JSON.stringify({
          transportVersion: 1,
          sessionId: "session-1",
          serverId: "server-1",
        }));
      }
      pollAttempt += 1;
      if (pollAttempt === 1) {
        return new Promise((_resolve, reject) => {
          init.signal?.addEventListener("abort", () => reject(abortError()), { once: true });
        });
      }
      if (pollAttempt === 2) {
        return response(200, JSON.stringify({
          frames: [{
            sequence: 1,
            message: { jsonrpc: "2.0", id: "rpc-1", result: 42 },
          }],
        }));
      }
      return new Promise(() => undefined);
    });
    const channel = createReliableHttpMessageChannel({
      endpointUrl: "http://127.0.0.1:4321",
      connectionId: "client-1",
      fetch,
      retryDelayMs: 0,
      receiveTimeoutMs: 5,
    });
    const received: RpcMessage[] = [];
    channel.subscribe((message) => received.push(message));

    await vi.waitFor(() => expect(received).toEqual([
      { jsonrpc: "2.0", id: "rpc-1", result: 42 },
    ]));
    channel.close();
  });
});

function decodeBase64(data: string) {
  return Uint8Array.from(atob(data), (character) => character.charCodeAt(0));
}

function response(status: number, body: string) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: async () => body,
  };
}

function abortError() {
  const error = new Error("aborted");
  error.name = "AbortError";
  return error;
}
