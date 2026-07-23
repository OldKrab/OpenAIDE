import { describe, expect, it, vi } from "vitest";
import type { RpcMessage, RpcMessageChannel } from "./rpcPeer";
import {
  createReliableBackendConnection,
  createReliableLocalHttpBackendConnection,
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

    const interruptedRequest = connection.request(TASK_LIST, { lifecycle: "open" });
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
      await expect(connection.request(TASK_LIST, { lifecycle: "open" })).resolves.toEqual({
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

  it("initializes a replacement session before polling it after mobile expiry", async () => {
    const transport = createExpiringSessionTransport("transport", "task", false, true);
    const connection = createReliableWebProxyBackendConnection({
      endpointUrl: "http://app-server.test/rpc",
      connectionId: "connection-1",
      fetch: transport.fetch,
      retryDelayMs: 1,
      heartbeatIntervalMs: 60_000,
    });
    await connection.initialize({
      clientInstanceId: "client-1" as ClientInstanceId,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: [], shell: [] },
    });

    await expect(connection.request(TASK_LIST, { lifecycle: "open" })).rejects.toThrow("HTTP 410");
    await vi.waitFor(() => expect(transport.initializedSessions()).toEqual([
      "session-1",
      "session-2",
    ]));
    await expect(connection.request(TASK_LIST, { lifecycle: "open" })).resolves.toEqual({
      tasks: [],
      revision: 2,
    });
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

    await connection.request(TASK_LIST, { lifecycle: "open" });

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
    const followUp = connection.request(TASK_LIST, { lifecycle: "open" });
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
    const queuedRequest = connection.request(TASK_LIST, { lifecycle: "open" });

    await vi.waitFor(() => expect(statuses.at(-1)).toBe("unavailable"));
    await expect(queuedRequest).rejects.toThrow("replacement initialization failed");
    connection.close();
  });

  it("replaces the App Server process endpoint behind one logical session", async () => {
    const transport = createExpiringSessionTransport("transport", "task", true);
    let replaceEndpoint: ((endpoint: {
      endpointUrl: string;
      authToken: string;
    }) => void) | undefined;
    const connection = createReliableLocalHttpBackendConnection({
      endpointUrl: "http://app-server-one.test/rpc",
      authToken: "token-1",
      connectionId: "connection-1",
      fetch: transport.fetch,
      retryDelayMs: 1,
      heartbeatIntervalMs: 60_000,
      subscribeToReplacement(listener) {
        replaceEndpoint = listener;
        return () => {
          replaceEndpoint = undefined;
        };
      },
    });
    const invalidations: string[] = [];
    const statuses: string[] = [];
    const projectSnapshots = vi.fn();
    connection.handleGenerationInvalidated(({ reason }) => invalidations.push(reason));
    connection.handleSessionStatus(({ status }) => statuses.push(status));
    await connection.initialize({
      clientInstanceId: "client-1" as ClientInstanceId,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: [], shell: [] },
    });
    connection.subscribeState({ kind: "projects" }, { onSnapshot: projectSnapshots });
    await vi.waitFor(() => expect(projectSnapshots).toHaveBeenCalledOnce());

    replaceEndpoint?.({
      endpointUrl: "http://app-server-two.test/rpc",
      authToken: "token-2",
    });

    await vi.waitFor(() => expect(transport.initializedSessions()).toEqual([
      "session-1",
      "session-2",
    ]));
    await vi.waitFor(() => expect(projectSnapshots).toHaveBeenCalledTimes(2));
    expect(transport.openedEndpoints()).toEqual([
      "http://app-server-one.test/rpc",
      "http://app-server-two.test/rpc",
    ]);
    expect(invalidations).toEqual(["appServerRestarted"]);
    expect(statuses.at(-1)).toBe("ready");
    connection.close();
  });

  it("applies a process replacement announced during expired-session recovery", async () => {
    const transport = createExpiringSessionTransport("transport", "task", true);
    let replaceEndpoint: ((endpoint: { endpointUrl: string; authToken: string }) => void) | undefined;
    const connection = createReliableLocalHttpBackendConnection({
      endpointUrl: "http://app-server-one.test/rpc",
      authToken: "token-1",
      connectionId: "connection-1",
      fetch: transport.fetch,
      retryDelayMs: 1,
      heartbeatIntervalMs: 60_000,
      subscribeToReplacement(listener) {
        replaceEndpoint = listener;
        return () => { replaceEndpoint = undefined; };
      },
    });
    const invalidations: string[] = [];
    connection.handleGenerationInvalidated(({ reason }) => invalidations.push(reason));
    await connection.initialize({
      clientInstanceId: "client-1" as ClientInstanceId,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: [], shell: [] },
    });
    transport.holdNextSessionOpen();

    const interrupted = connection.request(TASK_LIST, { lifecycle: "open" });
    void interrupted.catch(() => undefined);
    await vi.waitFor(() => expect(transport.openedSessions()).toBe(2));
    replaceEndpoint?.({
      endpointUrl: "http://app-server-two.test/rpc",
      authToken: "token-2",
    });
    transport.resolveHeldSessionOpen();

    await vi.waitFor(() => expect(transport.openedEndpoints()).toEqual([
      "http://app-server-one.test/rpc",
      "http://app-server-one.test/rpc",
      "http://app-server-two.test/rpc",
    ]));
    await vi.waitFor(() => expect(transport.initializedSessions()).toEqual([
      "session-1",
      "session-3",
    ]));
    expect(invalidations).toEqual(["httpSessionExpired"]);
    await expect(interrupted).rejects.toThrow();
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

    await expect(connection.request(TASK_LIST, { lifecycle: "open" })).rejects.toThrow("HTTP 409");
    expect(transport.openedSessions()).toBe(1);
    expect(invalidations).not.toHaveBeenCalled();
    connection.close();
  });
});

function createExpiringSessionTransport(
  expiry: "transport" | "client",
  trigger: "task" | "heartbeat" = "task",
  serverByEndpoint = false,
  rejectReplacementPollBeforeInitialize = false,
) {
  const frames = new Map<string, Array<{ sequence: number; message: RpcMessage }>>();
  const initialized: string[] = [];
  const taskLists: string[] = [];
  const permissionResponses: string[] = [];
  const heartbeats: string[] = [];
  const subscriptions: string[] = [];
  const openedEndpoints: string[] = [];
  const sessionServers = new Map<string, string>();
  let freezeNextReceive = false;
  let expireReplayOnNextReceive = false;
  let rejectAcknowledgementOnNextReceive = false;
  let failNextReplacementInitialization = false;
  let rejectedAcknowledgementCount = 0;
  let frozenPollCount = 0;
  let opened = 0;
  let holdNextSessionOpen = false;
  let heldSessionOpen: { resolve: (value: ReturnType<typeof response>) => void; value: ReturnType<typeof response> }
    | undefined;
  let heldSubscriptionKind: "agents" | "projects" | undefined;
  let heldSubscription: { id: string | number; sessionId: string; scope: { kind: "agents" | "projects" } }
    | undefined;

  return {
    fetch: vi.fn<ReliableHttpFetch>(async (input, init) => {
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
        if (
          rejectReplacementPollBeforeInitialize
          && sessionId !== "session-1"
          && !initialized.includes(sessionId)
        ) {
          return response(410, "client is not initialized");
        }
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
        const endpoint = String(input);
        openedEndpoints.push(endpoint);
        const serverId = serverByEndpoint && endpoint.includes("server-two")
          ? "server-2"
          : "server-1";
        const sessionId = `session-${opened}`;
        sessionServers.set(sessionId, serverId);
        const openedResponse = response(200, {
          transportVersion: 1,
          sessionId,
          serverId,
        });
        if (holdNextSessionOpen) {
          holdNextSessionOpen = false;
          return new Promise((resolve) => {
            heldSessionOpen = { resolve, value: openedResponse };
          });
        }
        return openedResponse;
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
          result: {
            result: initializeResult(
              sessionId === "session-1" ? "cursor-1" : "cursor-2",
              sessionServers.get(sessionId) ?? "server-1",
            ),
          },
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
    openedEndpoints: () => openedEndpoints,
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
    holdNextSessionOpen() {
      holdNextSessionOpen = true;
    },
    resolveHeldSessionOpen() {
      if (!heldSessionOpen) throw new Error("No held session open");
      heldSessionOpen.resolve(heldSessionOpen.value);
      heldSessionOpen = undefined;
    },
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

function initializeResult(cursor = "cursor-1", serverId = "server-1") {
  return {
    snapshot: {
      cursor,
      server: { serverId, protocolVersion: "v1", capabilities: {} },
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
