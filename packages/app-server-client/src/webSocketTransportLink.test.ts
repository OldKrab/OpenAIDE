import { afterEach, describe, expect, it, vi } from "vitest";
import { createDiagnosticsLogger } from "./diagnostics";
import {
  CLIENT_INITIALIZE,
  TASK_LIST,
  type ClientInstanceId,
} from "./generated/protocol";
import { createReliableLocalHttpBackendConnection } from "./reliableBackendConnection";
import { createReliableSessionChannel } from "./reliableSessionChannel";
import type { RpcMessage } from "./rpcPeer";
import { transportLinkCloseKind } from "./transportLink";
import {
  createWebSocketLinkOpener,
  type TransportWebSocket,
  type WebSocketTransportLinkOptions,
} from "./webSocketTransportLink";

type WireMessage = Record<string, unknown> & { type: string };

class FakeSocket implements TransportWebSocket {
  readonly sent: WireMessage[] = [];
  closedWith: { code?: number; reason?: string } | undefined;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code: number }) => void) | null = null;
  onerror: (() => void) | null = null;

  constructor(readonly url: string) {}

  send(data: string) {
    if (this.closedWith) throw new Error("socket is closed");
    this.sent.push(JSON.parse(data) as WireMessage);
  }

  close(code?: number, reason?: string) {
    this.closedWith = {
      ...(code === undefined ? {} : { code }),
      ...(reason === undefined ? {} : { reason }),
    };
  }

  receive(message: WireMessage) {
    this.onmessage?.({ data: JSON.stringify(message) });
  }

  /** The server accepts the hello it was sent. */
  accept(session: { sessionId?: string; serverId?: string; receivedThrough?: number } = {}) {
    this.onopen?.();
    this.receive({
      type: "ready",
      transportVersion: 1,
      sessionId: session.sessionId ?? "session-1",
      serverId: session.serverId ?? "server-1",
      receivedThrough: session.receivedThrough ?? 0,
    });
  }

  /** The socket ends with the given close code; 1006 is a lost connection. */
  end(code = 1006) {
    this.closedWith ??= {};
    this.onclose?.({ code });
  }

  sentOfType(type: string) {
    return this.sent.filter((message) => message.type === type);
  }
}

function harness(overrides: Partial<WebSocketTransportLinkOptions> = {}) {
  const sockets: FakeSocket[] = [];
  const channel = createReliableSessionChannel({
    connectionId: "client-1",
    retryDelayMs: 0,
    openLink: createWebSocketLinkOpener({
      endpointUrl: "http://127.0.0.1:4321/rpc",
      connectionId: "client-1",
      ackDelayMs: 0,
      createSocket(url) {
        const socket = new FakeSocket(url);
        sockets.push(socket);
        return socket;
      },
      ...overrides,
    }),
  });
  const received: RpcMessage[] = [];
  const errors: unknown[] = [];
  channel.subscribe((message) => received.push(message));
  channel.subscribeErrors?.((error) => errors.push(error));
  const socketAt = async (index: number) => {
    await vi.waitFor(() => expect(sockets.length).toBeGreaterThan(index));
    return sockets[index] as FakeSocket;
  };
  return { channel, sockets, received, errors, socketAt };
}

const request = (id: string): RpcMessage => ({ jsonrpc: "2.0", id, method: TASK_LIST, params: {} });
const result = (id: string): RpcMessage => ({ jsonrpc: "2.0", id, result: {} });

describe("WebSocket transport link", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("authenticates in the hello and exchanges sequenced frames on one socket", async () => {
    const { channel, sockets, received, socketAt } = harness({ authToken: "token-1" });
    channel.send(request("rpc-1"));
    const socket = await socketAt(0);
    socket.accept();

    await expect(channel.ready()).resolves.toEqual({ serverId: "server-1" });
    expect(socket.url).toBe("ws://127.0.0.1:4321/rpc?connectionId=client-1");
    expect(socket.url).not.toContain("token-1");
    expect(socket.sent[0]).toEqual({
      type: "hello",
      transportVersion: 1,
      connectionId: "client-1",
      receivedThrough: 0,
      authToken: "token-1",
    });
    expect(socket.sentOfType("frame")).toEqual([
      { type: "frame", sequence: 1, message: request("rpc-1") },
    ]);

    socket.receive({ type: "ack", through: 1 });
    socket.receive({ type: "frame", sequence: 1, message: result("rpc-1") });
    socket.receive({ type: "frame", sequence: 2, message: result("rpc-2") });

    expect(received).toEqual([result("rpc-1"), result("rpc-2")]);
    await vi.waitFor(() => expect(socket.sentOfType("ack").at(-1)).toEqual({ type: "ack", through: 2 }));
    expect(socket.sentOfType("ack")).toHaveLength(1);
    expect(sockets).toHaveLength(1);
    channel.close();
    expect(socket.closedWith?.code).toBe(1000);
  });

  it("resumes the session on a new socket and resends only what the server lacks", async () => {
    const { channel, received, errors, socketAt } = harness();
    const first = await socketAt(0);
    first.accept();
    await channel.ready();
    channel.send(request("rpc-1"));
    channel.send(request("rpc-2"));
    channel.send(request("rpc-3"));
    first.receive({ type: "ack", through: 1 });
    first.receive({ type: "frame", sequence: 1, message: result("rpc-1") });

    first.end();

    const second = await socketAt(1);
    second.onopen?.();
    expect(second.sent[0]).toEqual({
      type: "hello",
      transportVersion: 1,
      connectionId: "client-1",
      sessionId: "session-1",
      receivedThrough: 1,
    });
    // The server holds the second frame although its acknowledgement was lost.
    second.receive({
      type: "ready", transportVersion: 1, sessionId: "session-1", serverId: "server-1", receivedThrough: 2,
    });
    await vi.waitFor(() => expect(second.sentOfType("frame")).toEqual([
      { type: "frame", sequence: 3, message: request("rpc-3") },
    ]));

    // A replayed frame the client already applied is not delivered twice.
    second.receive({ type: "frame", sequence: 1, message: result("rpc-1") });
    second.receive({ type: "frame", sequence: 2, message: result("rpc-2") });
    expect(received).toEqual([result("rpc-1"), result("rpc-2")]);
    expect(errors).toEqual([]);
    channel.close();
  });

  it("retries a resume that cannot reach the server", async () => {
    const { channel, errors, socketAt } = harness();
    (await socketAt(0)).accept();
    await channel.ready();
    channel.send(request("rpc-1"));

    (await socketAt(0)).end();
    (await socketAt(1)).end();
    const third = await socketAt(2);
    third.accept();

    await vi.waitFor(() => expect(third.sentOfType("frame")).toHaveLength(1));
    expect(errors).toEqual([]);
    channel.close();
  });

  it.each([
    [4410, "sessionExpired"],
    [4409, "replayExpired"],
    [4403, "rejected"],
    [4000, "rejected"],
  ])("reports close code %i as %s without reopening", async (code, kind) => {
    const { channel, sockets, errors, socketAt } = harness();
    const socket = await socketAt(0);
    socket.accept();
    await channel.ready();

    socket.end(code);

    expect(errors).toHaveLength(1);
    expect(transportLinkCloseKind(errors[0])).toBe(kind);
    expect(() => channel.send(request("rpc-1"))).toThrow();
    expect(sockets).toHaveLength(1);
    channel.close();
  });

  it("ends the channel when the first socket never opens", async () => {
    const { channel, errors, socketAt } = harness();
    const ready = channel.ready();
    const rejected = expect(ready).rejects.toThrow("WebSocket link ended");

    (await socketAt(0)).end();

    await rejected;
    expect(errors).toHaveLength(1);
    channel.close();
  });

  it("replaces a socket that stops answering pings", async () => {
    vi.useFakeTimers();
    const { channel, sockets, errors } = harness({ pingIntervalMs: 1_000, pongTimeoutMs: 500 });
    const first = sockets[0] as FakeSocket;
    first.accept();
    await channel.ready();

    await vi.advanceTimersByTimeAsync(1_000);
    expect(first.sentOfType("ping")).toHaveLength(1);
    first.receive({ type: "pong" });
    await vi.advanceTimersByTimeAsync(600);
    expect(sockets).toHaveLength(1);

    await vi.advanceTimersByTimeAsync(400);
    expect(first.sentOfType("ping")).toHaveLength(2);
    await vi.advanceTimersByTimeAsync(500);

    expect(first.closedWith).toEqual({ code: 1000, reason: "pong_timeout" });
    expect(sockets).toHaveLength(2);
    expect(errors).toEqual([]);
    channel.close();
  });

  it("answers a server ping and probes the socket when the runtime wakes", async () => {
    vi.useFakeTimers();
    let wake: (() => void) | undefined;
    const { channel, sockets } = harness({
      wakePongTimeoutMs: 200,
      subscribeToWake(listener) {
        wake = listener;
        return () => {
          wake = undefined;
        };
      },
    });
    const first = sockets[0] as FakeSocket;
    first.accept();
    await channel.ready();

    first.receive({ type: "ping" });
    expect(first.sentOfType("pong")).toHaveLength(1);

    wake?.();
    expect(first.sentOfType("ping")).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(200);

    expect(sockets).toHaveLength(2);
    channel.close();
    expect(wake).toBeUndefined();
  });

  it("logs the link lifecycle without the token or the address", async () => {
    const sink = { info: vi.fn(), warn: vi.fn(), error: vi.fn() };
    const { channel, socketAt } = harness({
      authToken: "token-1",
      logger: createDiagnosticsLogger("test", sink),
    });
    const socket = await socketAt(0);
    socket.accept();
    await channel.ready();
    socket.end(4410);

    const lines = [...sink.info.mock.calls, ...sink.warn.mock.calls].map(([line]) => String(line));
    const events = lines.map((line) => (JSON.parse(line) as { event: string }).event);
    expect(events).toEqual([
      "reliable_websocket_open_started",
      "reliable_websocket_open_completed",
      "reliable_websocket_closed",
    ]);
    expect(lines.at(-1)).toContain('"close_code":4410');
    expect(lines.join("\n")).not.toMatch(/token-1|127\.0\.0\.1/);
    channel.close();
  });
});

describe("ReliableBackendConnection over WebSocket", () => {
  const initializeParams = {
    clientInstanceId: "client-1" as ClientInstanceId,
    shell: { kind: "web" as const },
    requestedSurface: { kind: "home" as const },
    capabilities: { protocol: [], shell: [] },
  };

  /** Plays the App Server on each socket: accepts the hello and answers initialize. */
  function serve(socket: FakeSocket, sessionId: string) {
    socket.accept({ sessionId });
    let sequence = 0;
    const answered = new Set<unknown>();
    const answer = () => {
      for (const frame of socket.sentOfType("frame")) {
        const message = frame.message as { id: string; method: string };
        if (answered.has(message.id)) continue;
        answered.add(message.id);
        socket.receive({ type: "ack", through: frame.sequence as number });
        socket.receive({
          type: "frame",
          sequence: ++sequence,
          message: {
            jsonrpc: "2.0",
            id: message.id,
            result: {
              result: message.method === CLIENT_INITIALIZE
                ? { snapshot: { server: { serverId: "server-1" } } }
                : { sessionId },
            },
          },
        });
      }
    };
    return answer;
  }

  it("initializes, and replaces an expired session with a fresh initialized one", async () => {
    const sockets: FakeSocket[] = [];
    const session = createReliableLocalHttpBackendConnection({
      endpointUrl: "http://127.0.0.1:4321",
      authToken: "token-1",
      connectionId: "client-1",
      transport: "webSocket",
      retryDelayMs: 0,
      heartbeatIntervalMs: 60_000,
      createWebSocket(url) {
        const socket = new FakeSocket(url);
        sockets.push(socket);
        return socket;
      },
    });
    const invalidations: string[] = [];
    const baselines: string[] = [];
    session.handleGenerationInvalidated((event) => invalidations.push(event.reason));
    session.handleRecoveryBaseline((event) => baselines.push(event.reason));

    const first = sockets[0] as FakeSocket;
    const answerFirst = serve(first, "session-1");
    const initializing = session.initialize(initializeParams);
    await vi.waitFor(() => {
      answerFirst();
      expect(first.sentOfType("frame")).toHaveLength(1);
    });
    await initializing;

    first.end(4410);

    await vi.waitFor(() => expect(sockets).toHaveLength(2));
    const second = sockets[1] as FakeSocket;
    expect(second.sent).toEqual([]);
    const answerSecond = serve(second, "session-2");
    // A replaced session starts over: no resume, and initialize is repeated.
    expect(second.sent[0]).not.toHaveProperty("sessionId");
    await vi.waitFor(() => {
      answerSecond();
      expect(baselines).toEqual(["httpSessionExpired"]);
    });
    expect(invalidations).toEqual(["httpSessionExpired"]);
    expect((second.sentOfType("frame")[0]?.message as { method: string }).method).toBe(CLIENT_INITIALIZE);

    const listing = session.request(TASK_LIST, { lifecycle: "open" });
    await vi.waitFor(() => {
      answerSecond();
      expect(second.sentOfType("frame")).toHaveLength(2);
    });
    await expect(listing).resolves.toEqual({ sessionId: "session-2" });
    session.close();
  });
});
