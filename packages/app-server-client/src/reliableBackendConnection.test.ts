import { describe, expect, it, vi } from "vitest";
import type { RpcMessage, RpcMessageChannel } from "./rpcPeer";
import {
  createReliableBackendConnection,
  createReliableWebProxyBackendConnection,
} from "./reliableBackendConnection";
import type { ReliableHttpFetch } from "./reliableHttpChannel";
import {
  CLIENT_HEARTBEAT,
  CLIENT_INITIALIZE,
  PERMISSION_REQUEST,
  TASK_LIST,
  type ClientInstanceId,
} from "./generated/protocol";

describe("ReliableBackendConnection", () => {
  it("resolves a typed App Server request from the peer receive channel", async () => {
    const sent: RpcMessage[] = [];
    const listeners = new Set<(message: RpcMessage) => void>();
    const channel: RpcMessageChannel = {
      send: (message) => sent.push(message),
      subscribe: (listener) => {
        listeners.add(listener);
        return () => listeners.delete(listener);
      },
    };
    const connection = createReliableBackendConnection({ channel });
    const initializing = connection.initialize({
      clientInstanceId: "client-1" as ClientInstanceId,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: [], shell: [] },
    });
    expect(sent[0]).toEqual(expect.objectContaining({
      jsonrpc: "2.0",
      id: "rpc-1",
      method: CLIENT_INITIALIZE,
    }));

    for (const receive of listeners) receive({
      jsonrpc: "2.0",
      id: "rpc-1",
      result: { result: initializeResult() },
    });

    await expect(initializing).resolves.toEqual(initializeResult());
    connection.close();
  });

  it("fans one App Server event notification out to every registered listener", () => {
    const listeners = new Set<(message: RpcMessage) => void>();
    const channel: RpcMessageChannel = {
      send: vi.fn(),
      subscribe: (listener) => {
        listeners.add(listener);
        return () => listeners.delete(listener);
      },
    };
    const connection = createReliableBackendConnection({ channel });
    const first = vi.fn();
    const second = vi.fn();
    connection.handleNotification("app/event", first);
    connection.handleNotification("app/event", second);
    const event = { cursor: "cursor-1" } as never;

    for (const receive of listeners) receive({
      jsonrpc: "2.0",
      method: "app/event",
      params: event,
    });

    expect(first).toHaveBeenCalledWith(event);
    expect(second).toHaveBeenCalledWith(event);
    connection.close();
  });

  it.each([
    {
      expiry: "transport" as const,
      failure: "HTTP 410",
      invalidationReason: "httpSessionExpired",
    },
    {
      expiry: "client" as const,
      failure: "client/initialize must succeed",
      invalidationReason: "clientLivenessExpired",
    },
  ])("reinitializes after $expiry expiry with safe replay semantics", async ({
    expiry,
    failure,
    invalidationReason,
  }) => {
    const transport = createExpiringSessionTransport(expiry);
    const connection = createReliableWebProxyBackendConnection({
      endpointUrl: "http://app-server.test/rpc",
      connectionId: "connection-1",
      fetch: transport.fetch,
      retryDelayMs: 1,
      heartbeatIntervalMs: 60_000,
    });
    const invalidations: string[] = [];
    connection.handleGenerationInvalidated(({ reason }) => invalidations.push(reason));
    const permissionHandler = vi.fn(async () => ({ optionId: "allow-once" }));
    connection.handleRequest(PERMISSION_REQUEST, permissionHandler);
    await connection.initialize({
      clientInstanceId: "client-1" as ClientInstanceId,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: [], shell: [] },
    });

    const interruptedRequest = connection.request(TASK_LIST, { archived: false });
    if (expiry === "transport") {
      await expect(interruptedRequest).rejects.toThrow(failure);
    } else {
      await expect(interruptedRequest).resolves.toEqual({ tasks: [], revision: 2 });
    }
    await vi.waitFor(() => expect(transport.openedSessions()).toBe(2));
    await vi.waitFor(() => expect(transport.initializedSessions()).toEqual([
      "session-1",
      "session-2",
    ]));

    if (expiry === "transport") {
      await expect(connection.request(TASK_LIST, { archived: false })).resolves.toEqual({
        tasks: [],
        revision: 2,
      });
    }
    transport.sendPermissionRequest("session-2");
    await vi.waitFor(() => expect(permissionHandler).toHaveBeenCalledOnce());
    await vi.waitFor(() => expect(transport.permissionResponseSessions()).toEqual(["session-2"]));
    expect(transport.taskListSessions()).toEqual(["session-1", "session-2"]);
    expect(invalidations).toEqual([invalidationReason]);
    connection.close();
  });

  it("recovers an expired mobile client from the resumed heartbeat", async () => {
    const transport = createExpiringSessionTransport("client", "heartbeat");
    const connection = createReliableWebProxyBackendConnection({
      endpointUrl: "http://app-server.test/rpc",
      connectionId: "connection-1",
      fetch: transport.fetch,
      retryDelayMs: 1,
      heartbeatIntervalMs: 5,
    });
    const permissionHandler = vi.fn(async () => ({ optionId: "allow-once" }));
    connection.handleRequest(PERMISSION_REQUEST, permissionHandler);
    await connection.initialize({
      clientInstanceId: "client-1" as ClientInstanceId,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: [], shell: [] },
    });

    await vi.waitFor(() => expect(transport.initializedSessions()).toEqual([
      "session-1",
      "session-2",
    ]));
    transport.sendPermissionRequest("session-2");
    await vi.waitFor(() => expect(permissionHandler).toHaveBeenCalledOnce());
    connection.close();
  });

  it("recovers an expired mobile client when wake restarts a frozen receive poll", async () => {
    let wake: (() => void) | undefined;
    const transport = createExpiringSessionTransport("client", "heartbeat");
    const connection = createReliableWebProxyBackendConnection({
      endpointUrl: "http://app-server.test/rpc",
      connectionId: "connection-1",
      fetch: transport.fetch,
      retryDelayMs: 1,
      heartbeatIntervalMs: 5,
      subscribeToWake(listener) {
        wake = listener;
        return () => {
          wake = undefined;
        };
      },
    });
    await connection.initialize({
      clientInstanceId: "client-1" as ClientInstanceId,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: [], shell: [] },
    });
    transport.freezeNextReceive();
    await vi.waitFor(() => expect(transport.frozenPolls()).toBe(1));
    await vi.waitFor(() => expect(transport.heartbeatSessions()).toContain("session-1"));

    wake?.();

    await vi.waitFor(() => expect(transport.initializedSessions()).toEqual([
      "session-1",
      "session-2",
    ]));
    connection.close();
  });

  it("invalidates stale state and reinitializes after server replay history expires", async () => {
    const transport = createExpiringSessionTransport("transport");
    const connection = createReliableWebProxyBackendConnection({
      endpointUrl: "http://app-server.test/rpc",
      connectionId: "connection-1",
      fetch: transport.fetch,
      retryDelayMs: 1,
      heartbeatIntervalMs: 60_000,
    });
    const invalidations: string[] = [];
    connection.handleGenerationInvalidated(({ reason }) => invalidations.push(reason));
    await connection.initialize({
      clientInstanceId: "client-1" as ClientInstanceId,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: [], shell: [] },
    });

    transport.expireReplayOnNextReceive();

    await vi.waitFor(() => expect(transport.initializedSessions()).toEqual([
      "session-1",
      "session-2",
    ]));
    expect(invalidations).toEqual(["serverReplayExpired"]);
    connection.close();
  });

  it("keeps an unrelated HTTP 409 terminal instead of discarding state", async () => {
    const transport = createExpiringSessionTransport("transport");
    const connection = createReliableWebProxyBackendConnection({
      endpointUrl: "http://app-server.test/rpc",
      connectionId: "connection-1",
      fetch: transport.fetch,
      retryDelayMs: 1,
      heartbeatIntervalMs: 60_000,
    });
    const invalidations = vi.fn();
    connection.handleGenerationInvalidated(invalidations);
    await connection.initialize({
      clientInstanceId: "client-1" as ClientInstanceId,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: [], shell: [] },
    });

    transport.rejectAcknowledgementOnNextReceive();
    await vi.waitFor(() => expect(transport.rejectedAcknowledgements()).toBe(1));

    await expect(connection.request(TASK_LIST, { archived: false })).rejects.toThrow("HTTP 409");
    expect(transport.openedSessions()).toBe(1);
    expect(invalidations).not.toHaveBeenCalled();
    connection.close();
  });
});

function createExpiringSessionTransport(
  expiry: "transport" | "client",
  trigger: "task" | "heartbeat" = "task",
) {
  const frames = new Map<string, Array<{ sequence: number; message: RpcMessage }>>();
  const initialized: string[] = [];
  const taskLists: string[] = [];
  const permissionResponses: string[] = [];
  const heartbeats: string[] = [];
  let freezeNextReceive = false;
  let expireReplayOnNextReceive = false;
  let rejectAcknowledgementOnNextReceive = false;
  let rejectedAcknowledgementCount = 0;
  let frozenPollCount = 0;
  let opened = 0;

  return {
    fetch: vi.fn<ReliableHttpFetch>(async (_input, init) => {
      if (init.method === "GET") {
        if (rejectAcknowledgementOnNextReceive) {
          rejectAcknowledgementOnNextReceive = false;
          rejectedAcknowledgementCount += 1;
          return response(409, "invalid acknowledgement");
        }
        if (expireReplayOnNextReceive) {
          expireReplayOnNextReceive = false;
          return response(409, { resyncRequired: true });
        }
        if (freezeNextReceive) {
          freezeNextReceive = false;
          frozenPollCount += 1;
          return new Promise((_resolve, reject) => {
            init.signal?.addEventListener("abort", () => reject(abortError()), { once: true });
          });
        }
        const sessionId = init.headers["X-OpenAIDE-Session-Id"] ?? "";
        const after = Number(init.headers["X-OpenAIDE-After"] ?? "0");
        const available = (frames.get(sessionId) ?? []).filter((frame) => frame.sequence > after);
        if (available.length > 0) return response(200, { frames: available });
        return response(204, "");
      }

      const body = JSON.parse(init.body ?? "{}") as {
        transport?: string;
        sessionId?: string;
        message?: RpcMessage;
      };
      if (body.transport === "open") {
        opened += 1;
        return response(200, {
          transportVersion: 1,
          sessionId: `session-${opened}`,
          serverId: "server-1",
        });
      }

      const sessionId = body.sessionId ?? "";
      const message = body.message;
      if (!message || !("id" in message)) {
        throw new Error("Test transport expected a JSON-RPC message with an id");
      }
      if (!("method" in message)) {
        permissionResponses.push(sessionId);
        return response(204, "");
      }
      if (message.method === CLIENT_INITIALIZE) {
        initialized.push(sessionId);
        enqueue(sessionId, {
          jsonrpc: "2.0",
          id: message.id,
          result: { result: initializeResult() },
        });
        return response(204, "");
      }
      if (message.method === CLIENT_HEARTBEAT) {
        heartbeats.push(sessionId);
        if (sessionId === "session-1" && trigger === "heartbeat") {
          enqueueNotInitialized(sessionId, message.id);
        } else {
          enqueue(sessionId, {
            jsonrpc: "2.0",
            id: message.id,
            result: { result: {} },
          });
        }
        return response(204, "");
      }
      if (message.method === TASK_LIST) {
        taskLists.push(sessionId);
        if (sessionId === "session-1") {
          if (expiry === "transport") return response(410, "session expired");
          enqueueNotInitialized(sessionId, message.id);
          return response(204, "");
        }
        enqueue(sessionId, {
          jsonrpc: "2.0",
          id: message.id,
          result: { result: { tasks: [], revision: 2 } },
        });
        return response(204, "");
      }
      throw new Error(`Unexpected test request: ${message.method}`);
    }),
    openedSessions: () => opened,
    frozenPolls: () => frozenPollCount,
    heartbeatSessions: () => heartbeats,
    initializedSessions: () => initialized,
    taskListSessions: () => taskLists,
    permissionResponseSessions: () => permissionResponses,
    freezeNextReceive() {
      freezeNextReceive = true;
    },
    expireReplayOnNextReceive() {
      expireReplayOnNextReceive = true;
    },
    rejectAcknowledgementOnNextReceive() {
      rejectAcknowledgementOnNextReceive = true;
    },
    rejectedAcknowledgements: () => rejectedAcknowledgementCount,
    sendPermissionRequest(sessionId: string) {
      enqueue(sessionId, {
        jsonrpc: "2.0",
        id: "permission-1",
        method: PERMISSION_REQUEST,
        params: {
          title: "Run command?",
          toolCall: { id: "tool-1", title: "Run command" },
          options: [{ optionId: "allow-once", name: "Allow", kind: "allowOnce" }],
        },
      });
    },
  };

  function enqueue(sessionId: string, message: RpcMessage) {
    const sessionFrames = frames.get(sessionId) ?? [];
    sessionFrames.push({ sequence: sessionFrames.length + 1, message });
    frames.set(sessionId, sessionFrames);
  }

  function enqueueNotInitialized(sessionId: string, id: string | number) {
    enqueue(sessionId, {
      jsonrpc: "2.0",
      id,
      error: {
        error: {
          code: "notInitialized",
          message: "client/initialize must succeed before product requests",
        },
      },
    });
  }
}

function response(status: number, body: unknown) {
  const text = typeof body === "string" ? body : JSON.stringify(body);
  return {
    ok: status >= 200 && status < 300,
    status,
    text: async () => text,
  };
}

function abortError() {
  const error = new Error("aborted");
  error.name = "AbortError";
  return error;
}

function initializeResult() {
  return {
    snapshot: {
      cursor: "cursor-1",
      server: { serverId: "server-1", protocolVersion: "v1", capabilities: {} },
      stateRoot: { stateRootId: "root-1" },
      client: {
        clientInstanceId: "client-1",
        shellKind: "web" as const,
        surface: { kind: "home" as const },
      },
      newTaskDefaults: { projectId: null, agentId: null, nativeSessionConfigOptions: [] },
      pendingRequests: [],
    },
  };
}
