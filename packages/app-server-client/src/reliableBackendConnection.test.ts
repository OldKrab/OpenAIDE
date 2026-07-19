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
  STATE_SUBSCRIBE,
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
    expect(new Set(transport.connectionIds()).size).toBe(2);
    expect(transport.clientInstanceIds()).toEqual(["client-1", "client-1"]);
    expect(invalidations).toEqual([invalidationReason]);
    connection.close();
  });

  it("prevents a stale old poll from draining replacement-generation deliveries", async () => {
    const transport = createConnectionDeliveryTransport();
    const connection = createReliableWebProxyBackendConnection({
      endpointUrl: "http://app-server.test/rpc",
      connectionId: "connection-1",
      fetch: transport.fetch,
      retryDelayMs: 0,
      heartbeatIntervalMs: 60_000,
    });
    const received = vi.fn();
    connection.handleNotification("app/event", received);
    await connection.initialize({
      clientInstanceId: "client-1" as ClientInstanceId,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: [], shell: [] },
    });

    await expect(connection.request(TASK_LIST, { archived: false })).rejects.toThrow("HTTP 410");
    await vi.waitFor(() => expect(transport.initializedSessions()).toEqual(["session-1", "session-2"]));
    transport.enqueueReplacementEvent();

    transport.releasePoll("session-1");
    await Promise.resolve();
    expect(received).not.toHaveBeenCalled();
    expect(transport.pendingDeliveryCount()).toBe(1);

    transport.releasePoll("session-2");
    await vi.waitFor(() => expect(received).toHaveBeenCalledOnce());
    expect(transport.deliveryConsumers()).toEqual(["session-2"]);
    connection.close();
  });

  it("bounds half-open close requests across repeated transport replacements", async () => {
    const transport = createExpiringSessionTransport("transport", "task", {
      expireEveryTask: true,
      halfOpenClose: true,
    });
    const connection = createReliableWebProxyBackendConnection({
      endpointUrl: "http://app-server.test/rpc",
      connectionId: "connection-1",
      fetch: transport.fetch,
      closeTimeoutMs: 5,
      retryDelayMs: 1,
      heartbeatIntervalMs: 60_000,
    });
    await connection.initialize({
      clientInstanceId: "client-1" as ClientInstanceId,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: [], shell: [] },
    });

    for (let replacement = 0; replacement < 8; replacement += 1) {
      await expect(connection.request(TASK_LIST, { archived: false })).rejects.toThrow("HTTP 410");
      await vi.waitFor(() => expect(transport.initializedSessions()).toHaveLength(replacement + 2));
      await vi.waitFor(() => expect(transport.abortedCloseRequests()).toBe(replacement + 1));
      expect(transport.pendingCloseRequests()).toBe(0);
    }

    expect(transport.maximumPendingCloseRequests()).toBeLessThanOrEqual(1);
    connection.close();
    await vi.waitFor(() => expect(transport.abortedCloseRequests()).toBe(9));
    expect(transport.pendingCloseRequests()).toBe(0);
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

  it("publishes the replacement initialization baseline after client liveness expires", async () => {
    const transport = createExpiringSessionTransport("client");
    const connection = createReliableWebProxyBackendConnection({
      endpointUrl: "http://app-server.test/rpc",
      connectionId: "connection-1",
      fetch: transport.fetch,
      retryDelayMs: 1,
      heartbeatIntervalMs: 60_000,
    });
    const baselines: Array<{ reason: string; cursor: string }> = [];
    connection.handleRecoveryBaseline(({ reason, result }) => {
      baselines.push({ reason, cursor: result.snapshot.cursor });
    });
    await connection.initialize({
      clientInstanceId: "client-1" as ClientInstanceId,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: [], shell: [] },
    });

    await connection.request(TASK_LIST, { archived: false });

    await vi.waitFor(() => expect(baselines).toEqual([{
      reason: "clientLivenessExpired",
      cursor: "cursor-2",
    }]));
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

  it("keeps requests behind the recovery barrier until every active scope has a baseline", async () => {
    const transport = createExpiringSessionTransport("transport");
    const connection = createReliableWebProxyBackendConnection({
      endpointUrl: "http://app-server.test/rpc",
      connectionId: "connection-1",
      fetch: transport.fetch,
      retryDelayMs: 1,
      heartbeatIntervalMs: 60_000,
    });
    const statuses: string[] = [];
    connection.handleSessionStatus(({ status }) => statuses.push(status));
    await connection.initialize({
      clientInstanceId: "client-1" as ClientInstanceId,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: [], shell: [] },
    });
    const projects = vi.fn();
    const agents = vi.fn();
    connection.subscribeState({ kind: "projects" }, { onSnapshot: projects });
    connection.subscribeState({ kind: "agents" }, { onSnapshot: agents });
    await vi.waitFor(() => expect(projects).toHaveBeenCalledOnce());
    await vi.waitFor(() => expect(agents).toHaveBeenCalledOnce());
    transport.holdNextSubscription("agents");

    transport.expireReplayOnNextReceive();
    await vi.waitFor(() => expect(statuses).toContain("recovering"));
    await vi.waitFor(() => expect(transport.subscriptionSessions()).toEqual([
      "session-1:projects",
      "session-1:agents",
      "session-2:projects",
      "session-2:agents",
    ]));
    const followUp = connection.request(TASK_LIST, { archived: false });
    await Promise.resolve();
    expect(transport.taskListSessions()).not.toContain("session-2");

    transport.resolveHeldSubscription();

    await expect(followUp).resolves.toEqual({ tasks: [], revision: 2 });
    expect(statuses.at(-1)).toBe("ready");
    connection.close();
  });

  it("settles the recovery barrier as unavailable when replacement initialization fails", async () => {
    const transport = createExpiringSessionTransport("transport");
    const connection = createReliableWebProxyBackendConnection({
      endpointUrl: "http://app-server.test/rpc",
      connectionId: "connection-1",
      fetch: transport.fetch,
      retryDelayMs: 1,
      heartbeatIntervalMs: 60_000,
    });
    const statuses: string[] = [];
    connection.handleSessionStatus(({ status }) => statuses.push(status));
    await connection.initialize({
      clientInstanceId: "client-1" as ClientInstanceId,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: [], shell: [] },
    });
    transport.failNextReplacementInitialization();

    transport.expireReplayOnNextReceive();
    await vi.waitFor(() => expect(statuses).toContain("recovering"));
    const queuedRequest = connection.request(TASK_LIST, { archived: false });

    await vi.waitFor(() => expect(statuses.at(-1)).toBe("unavailable"));
    await expect(queuedRequest).rejects.toThrow("replacement initialization failed");
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
  options: { expireEveryTask?: boolean; halfOpenClose?: boolean } = {},
) {
  const frames = new Map<string, Array<{ sequence: number; message: RpcMessage }>>();
  const initialized: string[] = [];
  const taskLists: string[] = [];
  const permissionResponses: string[] = [];
  const heartbeats: string[] = [];
  const subscriptions: string[] = [];
  const connectionIds: string[] = [];
  const clientInstanceIds: string[] = [];
  let freezeNextReceive = false;
  let expireReplayOnNextReceive = false;
  let rejectAcknowledgementOnNextReceive = false;
  let failNextReplacementInitialization = false;
  let rejectedAcknowledgementCount = 0;
  let frozenPollCount = 0;
  let opened = 0;
  let pendingCloseRequests = 0;
  let maximumPendingCloseRequests = 0;
  let abortedCloseRequests = 0;
  let heldSubscriptionKind: "agents" | "projects" | undefined;
  let heldSubscription: { id: string | number; sessionId: string; scope: { kind: "agents" | "projects" } }
    | undefined;

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
        connectionIds.push(init.headers["X-OpenAIDE-Connection-Id"] ?? "");
        return response(200, {
          transportVersion: 1,
          sessionId: `session-${opened}`,
          serverId: "server-1",
        });
      }
      if (body.transport === "close") {
        if (options.halfOpenClose) {
          pendingCloseRequests += 1;
          maximumPendingCloseRequests = Math.max(maximumPendingCloseRequests, pendingCloseRequests);
          return new Promise((_resolve, reject) => {
            init.signal?.addEventListener("abort", () => {
              pendingCloseRequests -= 1;
              abortedCloseRequests += 1;
              reject(abortError());
            }, { once: true });
          });
        }
        return response(200, { sessionId: body.sessionId });
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
        clientInstanceIds.push((message.params as { clientInstanceId: string }).clientInstanceId);
        if (sessionId !== "session-1" && failNextReplacementInitialization) {
          failNextReplacementInitialization = false;
          enqueue(sessionId, {
            jsonrpc: "2.0",
            id: message.id,
            error: {
              error: {
                code: "internal",
                message: "replacement initialization failed",
              },
            },
          });
          return response(204, "");
        }
        enqueue(sessionId, {
          jsonrpc: "2.0",
          id: message.id,
          result: { result: initializeResult(sessionId === "session-1" ? "cursor-1" : "cursor-2") },
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
        if (options.expireEveryTask || sessionId === "session-1") {
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
      if (message.method === STATE_SUBSCRIBE) {
        const scope = (message.params as { scope: { kind: "agents" | "projects" } }).scope;
        subscriptions.push(`${sessionId}:${scope.kind}`);
        if (sessionId === "session-2" && heldSubscriptionKind === scope.kind) {
          heldSubscriptionKind = undefined;
          heldSubscription = { id: message.id, sessionId, scope };
          return response(204, "");
        }
        enqueueSubscription(sessionId, message.id, scope);
        return response(204, "");
      }
      throw new Error(`Unexpected test request: ${message.method}`);
    }),
    openedSessions: () => opened,
    connectionIds: () => connectionIds,
    clientInstanceIds: () => clientInstanceIds,
    pendingCloseRequests: () => pendingCloseRequests,
    maximumPendingCloseRequests: () => maximumPendingCloseRequests,
    abortedCloseRequests: () => abortedCloseRequests,
    frozenPolls: () => frozenPollCount,
    heartbeatSessions: () => heartbeats,
    initializedSessions: () => initialized,
    taskListSessions: () => taskLists,
    subscriptionSessions: () => subscriptions,
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
    failNextReplacementInitialization() {
      failNextReplacementInitialization = true;
    },
    rejectedAcknowledgements: () => rejectedAcknowledgementCount,
    holdNextSubscription(kind: "agents" | "projects") {
      heldSubscriptionKind = kind;
    },
    resolveHeldSubscription() {
      if (!heldSubscription) throw new Error("No held subscription");
      enqueueSubscription(heldSubscription.sessionId, heldSubscription.id, heldSubscription.scope);
      heldSubscription = undefined;
    },
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

  function enqueueSubscription(
    sessionId: string,
    id: string | number,
    scope: { kind: "agents" | "projects" },
  ) {
    enqueue(sessionId, {
      jsonrpc: "2.0",
      id,
      result: {
        result: scope.kind === "projects"
          ? { cursor: `cursor-${sessionId}`, scope, snapshot: { kind: "projects", projects: { projects: [] } } }
          : { cursor: `cursor-${sessionId}`, scope, snapshot: { kind: "agents", agents: { agents: [] } } },
      },
    });
  }
}

/** Models the server delivery queue at its real connection-id ownership boundary. */
function createConnectionDeliveryTransport() {
  const sessions = new Map<string, string>();
  const frames = new Map<string, Array<{ sequence: number; message: RpcMessage }>>();
  const pendingPolls = new Map<string, { after: number; resolve(value: ReturnType<typeof response>): void }>();
  const deliveries = new Map<string, RpcMessage[]>();
  const initialized: string[] = [];
  const consumers: string[] = [];
  let opened = 0;

  const fetch = vi.fn<ReliableHttpFetch>(async (_input, init) => {
    if (init.method === "GET") {
      const sessionId = init.headers["X-OpenAIDE-Session-Id"] ?? "";
      const after = Number(init.headers["X-OpenAIDE-After"] ?? "0");
      return new Promise((resolve) => {
        pendingPolls.set(sessionId, { after, resolve });
        flushSessionFrames(sessionId);
      });
    }

    const body = JSON.parse(init.body ?? "{}") as {
      transport?: string;
      sessionId?: string;
      message?: RpcMessage;
    };
    if (body.transport === "open") {
      opened += 1;
      const sessionId = `session-${opened}`;
      sessions.set(sessionId, init.headers["X-OpenAIDE-Connection-Id"] ?? "");
      return response(200, { transportVersion: 1, sessionId, serverId: "server-1" });
    }
    if (body.transport === "close") {
      return response(200, { sessionId: body.sessionId });
    }
    const sessionId = body.sessionId ?? "";
    const message = body.message;
    if (!message || !("id" in message) || !("method" in message)) return response(204, "");
    if (message.method === CLIENT_INITIALIZE) {
      initialized.push(sessionId);
      enqueueFrame(sessionId, {
        jsonrpc: "2.0",
        id: message.id,
        result: { result: initializeResult(sessionId === "session-1" ? "cursor-1" : "cursor-2") },
      });
      return response(204, "");
    }
    if (message.method === TASK_LIST && sessionId === "session-1") {
      return response(410, "expired");
    }
    throw new Error(`Unexpected test request: ${message.method}`);
  });

  return {
    fetch,
    initializedSessions: () => initialized,
    enqueueReplacementEvent() {
      const connectionId = sessions.get("session-2") ?? "";
      deliveries.set(connectionId, [{
        jsonrpc: "2.0",
        method: "app/event",
        params: { cursor: "replacement-event" },
      } as RpcMessage]);
    },
    releasePoll(sessionId: string) {
      const connectionId = sessions.get(sessionId) ?? "";
      const queued = deliveries.get(connectionId) ?? [];
      if (queued.length > 0) {
        consumers.push(sessionId);
        deliveries.delete(connectionId);
        for (const message of queued) enqueueFrame(sessionId, message, false);
      }
      flushSessionFrames(sessionId, true);
    },
    pendingDeliveryCount: () => [...deliveries.values()].reduce((count, queued) => count + queued.length, 0),
    deliveryConsumers: () => consumers,
  };

  function enqueueFrame(sessionId: string, message: RpcMessage, flush = true) {
    const queued = frames.get(sessionId) ?? [];
    queued.push({ sequence: queued.length + 1, message });
    frames.set(sessionId, queued);
    if (flush) flushSessionFrames(sessionId);
  }

  function flushSessionFrames(sessionId: string, emptyWhenMissing = false) {
    const pending = pendingPolls.get(sessionId);
    if (!pending) return;
    const available = (frames.get(sessionId) ?? []).filter((frame) => frame.sequence > pending.after);
    if (available.length === 0 && !emptyWhenMissing) return;
    pendingPolls.delete(sessionId);
    pending.resolve(response(available.length > 0 ? 200 : 204, available.length > 0 ? { frames: available } : ""));
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

function initializeResult(cursor = "cursor-1") {
  return {
    snapshot: {
      cursor,
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
