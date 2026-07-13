import { describe, expect, it, vi } from "vitest";
import {
  CLIENT_CAPABILITIES_CHANGED,
  CLIENT_HEARTBEAT,
  CLIENT_INITIALIZE,
  PERMISSION_REQUEST,
  QUESTION_REQUEST,
  STATE_SUBSCRIBE,
  type AppServerEvent,
  type ClientInstanceId,
  type InitializeParams,
  type InitializeResult,
  type RequestId,
} from "./generated/protocol";
import {
  createLocalHttpBackendConnection,
  createWebProxyBackendConnection,
  type LocalHttpFetch,
} from "./localHttpConnection";
import { BackendReplicaChangedError } from "./backendReplicaChangedError";
import { AppServerProtocolError } from "./protocolError";

describe("LocalHttpBackendConnection", () => {
  it("does not restart background work when initialization resolves after close", async () => {
    vi.useFakeTimers();
    try {
      const initializeResponse = deferred<ReturnType<typeof fetchResponse>>();
      let eventStreamAttempts = 0;
      const fetch = vi.fn<LocalHttpFetch>(async (_input, init) => {
        if (init.headers.Accept === "text/event-stream") {
          eventStreamAttempts += 1;
          return eventStreamResponse([]);
        }
        return initializeResponse.promise;
      });
      const connection = createLocalHttpBackendConnection({
        ...connectionOptions(fetch),
        heartbeatIntervalMs: 25,
      });

      const initializing = connection.initialize(initializeParams());
      connection.close();
      initializeResponse.resolve(fetchResponse([
        response("local-http-request-1", { result: initializeResult() }),
      ]));

      await expect(initializing).rejects.toThrow("Backend connection closed");
      await vi.advanceTimersByTimeAsync(50);

      expect(eventStreamAttempts).toBe(0);
      expect(fetch).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("initializes over LocalHttp with bearer auth and stable connection id", async () => {
    const fetch = fetchReturning([
      response("local-http-request-1", { result: initializeResult() }),
    ]);
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));

    await expect(connection.initialize(initializeParams())).resolves.toEqual(initializeResult());

    expect(fetch).toHaveBeenCalledWith(
      "http://127.0.0.1:4321",
      expect.objectContaining({
        method: "POST",
        headers: {
          Authorization: "Bearer token-1",
          "Content-Type": "application/json",
          "X-OpenAIDE-Connection-Id": "client-1",
        },
        body: JSON.stringify({
          jsonrpc: "2.0",
          id: "local-http-request-1",
          method: CLIENT_INITIALIZE,
          params: initializeParams(),
        }),
      }),
    );
  });

  it("initializes over WebProxy without exposing bearer auth from the browser", async () => {
    const fetch = fetchReturning([
      response("local-http-request-1", { result: initializeResult() }),
    ]);
    const connection = createWebProxyBackendConnection({
      endpointUrl: "/__openaide-app-server/probe",
      connectionId: "client-1",
      fetch,
    });

    await connection.initialize(initializeParams());

    expect(fetch).toHaveBeenCalledWith(
      "/__openaide-app-server/probe",
      expect.objectContaining({
        headers: {
          "Content-Type": "application/json",
          "X-OpenAIDE-Connection-Id": "client-1",
        },
      }),
    );
  });

  it("requires initialize before product requests", async () => {
    const connection = createLocalHttpBackendConnection(connectionOptions(fetchReturning([])));

    await expect(connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } }))
      .rejects.toThrow("not initialized");
  });

  it("delivers app events from request responses", async () => {
    const event = appEvent();
    const fetch = fetchSequence([
      [response("local-http-request-1", { result: initializeResult() })],
      [
        response("local-http-request-2", {
          result: {
            cursor: "cursor-2",
            scope: { kind: "projects" },
            snapshot: { kind: "projects", projects: { projects: [] } },
          },
        }),
        { jsonrpc: "2.0", method: "app/event", params: event },
      ],
    ]);
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));
    const events: AppServerEvent[] = [];

    connection.events((received) => events.push(received));
    await connection.initialize(initializeParams());
    await connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } });

    expect(events).toEqual([event]);
  });

  it("delivers pushed events while the originating request remains unresolved", async () => {
    const event = appEvent();
    let requestResolved = false;
    const fetch = vi.fn<LocalHttpFetch>(async (_input, init) => {
      if (init.headers.Accept === "text/event-stream") {
        return eventStreamResponse([event]);
      }
      const request = JSON.parse(init.body) as { id: string; method: string };
      if (request.method === CLIENT_INITIALIZE) {
        return fetchResponse([response(request.id, { result: initializeResult() })]);
      }
      return new Promise(() => undefined);
    });
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));
    const events: AppServerEvent[] = [];

    connection.events((received) => events.push(received));
    await connection.initialize(initializeParams());
    void connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } }).then(() => {
      requestResolved = true;
    });

    await vi.waitFor(() => expect(events).toEqual([event]));
    expect(requestResolved).toBe(false);
    connection.close();
  });

  it("reconnects the event stream after a transient stream close", async () => {
    vi.useFakeTimers();
    const event = appEvent();
    let streamAttempt = 0;
    const fetch = vi.fn<LocalHttpFetch>(async (_input, init) => {
      if (init.headers.Accept === "text/event-stream") {
        streamAttempt += 1;
        return streamAttempt === 1 ? closedEventStreamResponse() : eventStreamResponse([event]);
      }
      const request = JSON.parse(init.body) as { id: string };
      return fetchResponse([response(request.id, { result: initializeResult() })]);
    });
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));
    const events: AppServerEvent[] = [];
    connection.events((received) => events.push(received));

    await connection.initialize(initializeParams());
    await vi.advanceTimersByTimeAsync(500);
    await vi.waitFor(() => expect(events).toEqual([event]));

    expect(streamAttempt).toBe(2);
    connection.close();
  });

  it("invalidates subscribed state after event-stream continuity is lost", async () => {
    vi.useFakeTimers();
    try {
      let streamAttempt = 0;
      const fetch = vi.fn<LocalHttpFetch>(async (_input, init) => {
        if (init.headers.Accept === "text/event-stream") {
          streamAttempt += 1;
          return streamAttempt === 1 ? closedEventStreamResponse() : eventStreamResponse([]);
        }
        const request = JSON.parse(init.body) as { id: string };
        return fetchResponse([response(request.id, { result: initializeResult() })]);
      });
      const connection = createLocalHttpBackendConnection(connectionOptions(fetch));
      const resets: unknown[] = [];

      connection.stateResets((reset) => resets.push(reset));
      await connection.initialize(initializeParams());
      await vi.waitFor(() => expect(resets).toEqual([
        { serverId: "server-1", stateRootId: "root-1" },
      ]));
      expect(streamAttempt).toBe(1);
      await vi.advanceTimersByTimeAsync(500);

      expect(streamAttempt).toBe(2);
      expect(resets).toHaveLength(1);
      connection.close();
    } finally {
      vi.useRealTimers();
    }
  });

  it("reinitializes before reconnecting when the server rejects the event stream", async () => {
    vi.useFakeTimers();
    try {
      let initializeAttempts = 0;
      let streamAttempts = 0;
      const fetch = vi.fn<LocalHttpFetch>(async (_input, init) => {
        if (init.headers.Accept === "text/event-stream") {
          streamAttempts += 1;
          return streamAttempts === 1
            ? { ok: false, status: 409, async text() { return ""; } }
            : eventStreamResponse([]);
        }
        initializeAttempts += 1;
        const request = JSON.parse(init.body) as { id: string };
        return fetchResponse([response(request.id, {
          result: initializeResult(initializeAttempts === 1 ? "server-1" : "server-2"),
        })]);
      });
      const connection = createLocalHttpBackendConnection(connectionOptions(fetch));
      const reset = vi.fn();
      connection.stateResets(reset);

      await connection.initialize(initializeParams());
      await vi.advanceTimersByTimeAsync(500);

      expect(initializeAttempts).toBe(2);
      expect(streamAttempts).toBe(2);
      expect(reset).toHaveBeenCalledOnce();
      expect(reset).toHaveBeenCalledWith({
        serverId: "server-2",
        stateRootId: "root-1",
      });
      connection.close();
    } finally {
      vi.useRealTimers();
    }
  });

  it("reinitializes without replaying a product request into the replacement replica", async () => {
    const fetch = fetchSequence([
      [response("local-http-request-1", { result: initializeResult() })],
      [protocolError("local-http-request-2", "notInitialized", "client/initialize must succeed before product requests")],
      [response("local-http-request-3", { result: initializeResult("server-2") })],
    ]);
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));
    const reset = vi.fn();
    connection.stateResets(reset);

    await connection.initialize(initializeParams());
    const error = await connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } })
      .catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(BackendReplicaChangedError);
    expect(error).not.toBeInstanceOf(AppServerProtocolError);
    expect(error).toMatchObject({
      name: "BackendReplicaChangedError",
      method: STATE_SUBSCRIBE,
      previousReplica: { serverId: "server-1", stateRootId: "root-1" },
      currentReplica: { serverId: "server-2", stateRootId: "root-1" },
    });

    expect(fetch.mock.calls
      .filter((call) => call[1].headers.Accept !== "text/event-stream")
      .map((call) => JSON.parse(String(call[1].body)).method)).toEqual([
      CLIENT_INITIALIZE,
      STATE_SUBSCRIBE,
      CLIENT_INITIALIZE,
    ]);
    expect(reset.mock.calls.map(([identity]) => identity)).toEqual([
      { serverId: "server-1", stateRootId: "root-1" },
      { serverId: "server-2", stateRootId: "root-1" },
    ]);
    connection.close();
  });

  it("uses the latest workspace roots to initialize the replacement replica", async () => {
    const fetch = fetchSequence([
      [response("local-http-request-1", { result: initializeResult() })],
      [response("local-http-request-2", {
        result: { projects: { projects: [] } },
      })],
      [protocolError(
        "local-http-request-3",
        "notInitialized",
        "client/initialize must succeed before product requests",
      )],
      [response("local-http-request-4", { result: initializeResult("server-2") })],
      [response("local-http-request-5", {
        result: {
          cursor: "cursor-3",
          scope: { kind: "projects" },
          snapshot: { kind: "projects", projects: { projects: [] } },
        },
      })],
    ]);
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));
    const initialParams = {
      ...initializeParams(),
      capabilities: { protocol: ["resync"] },
      workspaceRoots: [{ path: "/workspace/alpha" }],
    } as InitializeParams;

    await connection.initialize(initialParams);
    await connection.request(CLIENT_CAPABILITIES_CHANGED, {
      capabilities: { protocol: ["requestResponses"] },
      workspaceRoots: [{ path: "/workspace/beta" }],
    });
    await expect(connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } }))
      .rejects.toBeInstanceOf(BackendReplicaChangedError);
    await expect(connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } }))
      .resolves.toMatchObject({ cursor: "cursor-3" });

    const requests = fetch.mock.calls
      .filter(([, init]) => init.headers.Accept !== "text/event-stream")
      .map(([, init]) => JSON.parse(init.body));
    expect(requests[3]).toMatchObject({
      method: CLIENT_INITIALIZE,
      params: {
        capabilities: { protocol: ["requestResponses"] },
        workspaceRoots: [{ path: "/workspace/beta" }],
      },
    });
    connection.close();
  });

  it("retries initialization on a later request after transient reinitialize failure", async () => {
    let initializeAttempts = 0;
    let subscribeAttempts = 0;
    const fetch = vi.fn<LocalHttpFetch>(async (_input, init) => {
      if (init.headers.Accept === "text/event-stream") return eventStreamResponse([]);
      const request = JSON.parse(init.body) as { id: string; method: string };
      if (request.method === CLIENT_INITIALIZE) {
        initializeAttempts += 1;
        if (initializeAttempts === 2) {
          return { ok: false, status: 503, async text() { return "temporarily unavailable"; } };
        }
        return fetchResponse([response(request.id, { result: initializeResult() })]);
      }
      subscribeAttempts += 1;
      if (subscribeAttempts === 1) {
        return fetchResponse([
          protocolError(request.id, "notInitialized", "client/initialize must succeed before product requests"),
        ]);
      }
      return fetchResponse([
        response(request.id, {
          result: {
            cursor: "cursor-2",
            scope: { kind: "projects" },
            snapshot: { kind: "projects", projects: { projects: [] } },
          },
        }),
      ]);
    });
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));

    await connection.initialize(initializeParams());
    await expect(connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } }))
      .rejects.toThrow("App Server request failed with HTTP 503");
    await expect(connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } }))
      .rejects.toBeInstanceOf(BackendReplicaChangedError);
    await expect(connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } }))
      .resolves.toMatchObject({ cursor: "cursor-2" });

    expect(initializeAttempts).toBe(3);
    expect(subscribeAttempts).toBe(2);
    connection.close();
  });

  it("invalidates subscribed state after reinitialize even when the replacement event stream stays down", async () => {
    let subscribeAttempts = 0;
    let initializeAttempts = 0;
    let streamAttempts = 0;
    const fetch = vi.fn<LocalHttpFetch>(async (_input, init) => {
      if (init.headers.Accept === "text/event-stream") {
        streamAttempts += 1;
        return streamAttempts === 1
          ? eventStreamResponse([])
          : { ok: false, status: 503, async text() { return "unavailable"; } };
      }
      const request = JSON.parse(init.body) as { id: string; method: string };
      if (request.method === CLIENT_INITIALIZE) {
        initializeAttempts += 1;
        return fetchResponse([response(request.id, {
          result: initializeResult(initializeAttempts === 1 ? "server-1" : "server-2"),
        })]);
      }
      subscribeAttempts += 1;
      if (subscribeAttempts === 1) {
        return fetchResponse([
          protocolError(request.id, "notInitialized", "client/initialize must succeed before product requests"),
        ]);
      }
      return fetchResponse([
        response(request.id, {
          result: {
            cursor: "cursor-2",
            scope: { kind: "projects" },
            snapshot: { kind: "projects", projects: { projects: [] } },
          },
        }),
      ]);
    });
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));
    const reset = vi.fn();
    connection.stateResets(reset);

    await connection.initialize(initializeParams());
    await expect(connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } }))
      .rejects.toBeInstanceOf(BackendReplicaChangedError);

    expect(streamAttempts).toBe(2);
    expect(reset).toHaveBeenCalledOnce();
    expect(reset).toHaveBeenCalledWith({ serverId: "server-2", stateRootId: "root-1" });

    connection.close();
  });

  it("coalesces reinitialization when concurrent requests discover the same restart", async () => {
    const replacementInitialize = deferred<ReturnType<typeof fetchResponse>>();
    let initializeAttempts = 0;
    let subscribeAttempts = 0;
    let replacementInitializeRequestId = "";
    const fetch = vi.fn<LocalHttpFetch>(async (_input, init) => {
      if (init.headers.Accept === "text/event-stream") return eventStreamResponse([]);
      const request = JSON.parse(init.body) as { id: string; method: string };
      if (request.method === CLIENT_INITIALIZE) {
        initializeAttempts += 1;
        if (initializeAttempts === 1) {
          return fetchResponse([response(request.id, { result: initializeResult() })]);
        }
        replacementInitializeRequestId = request.id;
        return replacementInitialize.promise;
      }
      subscribeAttempts += 1;
      return fetchResponse([
        protocolError(request.id, "notInitialized", "client/initialize must succeed before product requests"),
      ]);
    });
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));
    const reset = vi.fn();
    connection.stateResets(reset);

    await connection.initialize(initializeParams());
    const requests = [
      connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } }).catch((error) => error),
      connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } }).catch((error) => error),
    ];
    await vi.waitFor(() => expect(initializeAttempts).toBe(2));
    replacementInitialize.resolve(fetchResponse([
      response(replacementInitializeRequestId, { result: initializeResult("server-2") }),
    ]));
    const errors = await Promise.all(requests);

    expect(errors).toEqual([
      expect.any(BackendReplicaChangedError),
      expect.any(BackendReplicaChangedError),
    ]);
    expect(initializeAttempts).toBe(2);
    expect(subscribeAttempts).toBe(2);
    expect(reset).toHaveBeenCalledOnce();
    expect(reset).toHaveBeenCalledWith({ serverId: "server-2", stateRootId: "root-1" });
    connection.close();
  });

  it("rejects an old-replica result that arrives after concurrent recovery", async () => {
    const delayedResult = deferred<ReturnType<typeof fetchResponse>>();
    let initializeAttempts = 0;
    let subscribeAttempts = 0;
    let delayedRequestId = "";
    const fetch = vi.fn<LocalHttpFetch>(async (_input, init) => {
      if (init.headers.Accept === "text/event-stream") return eventStreamResponse([]);
      const request = JSON.parse(init.body) as { id: string; method: string };
      if (request.method === CLIENT_INITIALIZE) {
        initializeAttempts += 1;
        return fetchResponse([response(request.id, {
          result: initializeResult(initializeAttempts === 1 ? "server-1" : "server-2"),
        })]);
      }
      subscribeAttempts += 1;
      if (subscribeAttempts === 1) {
        delayedRequestId = request.id;
        return delayedResult.promise;
      }
      return fetchResponse([
        protocolError(request.id, "notInitialized", "client/initialize must succeed before product requests"),
      ]);
    });
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));

    await connection.initialize(initializeParams());
    const staleRequest = connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } })
      .catch((error) => error);
    const recoveryRequest = connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } })
      .catch((error) => error);

    await expect(recoveryRequest).resolves.toBeInstanceOf(BackendReplicaChangedError);
    delayedResult.resolve(fetchResponse([
      response(delayedRequestId, {
        result: {
          cursor: "cursor-from-server-1",
          scope: { kind: "projects" },
          snapshot: { kind: "projects", projects: { projects: [] } },
        },
      }),
    ]));

    await expect(staleRequest).resolves.toBeInstanceOf(BackendReplicaChangedError);
    expect(initializeAttempts).toBe(2);
    expect(subscribeAttempts).toBe(2);
    connection.close();
  });

  it("delivers backend-initiated server requests from response batches", async () => {
    const fetch = fetchSequence([
      [response("local-http-request-1", { result: initializeResult() })],
      [
        response("local-http-request-2", {
          result: {
            cursor: "cursor-2",
            scope: { kind: "projects" },
            snapshot: { kind: "projects", projects: { projects: [] } },
          },
        }),
        {
          jsonrpc: "2.0",
          id: "server-request-1",
          scope: { kind: "task", taskId: "task-1" },
          method: PERMISSION_REQUEST,
          params: {
            title: "Allow?",
            toolCall: { id: "tool-1", title: "Edit file" },
            options: [{ optionId: "allow-once", name: "Allow once", kind: "allowOnce" }],
          },
        },
      ],
    ]);
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));
    const requests: unknown[] = [];

    connection.serverRequests((request) => requests.push(request));
    await connection.initialize(initializeParams());
    await connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } });

    expect(requests).toEqual([
      {
        requestId: "server-request-1",
        scope: { kind: "task", taskId: "task-1" },
        method: PERMISSION_REQUEST,
        params: {
          title: "Allow?",
          toolCall: { id: "tool-1", title: "Edit file" },
          options: [{ optionId: "allow-once", name: "Allow once", kind: "allowOnce" }],
        },
      },
    ]);
  });

  it("delivers typed question requests without breaking the connection", async () => {
    const fetch = fetchSequence([
      [response("local-http-request-1", { result: initializeResult() })],
      [
        response("local-http-request-2", {
          result: {
            cursor: "cursor-2",
            scope: { kind: "projects" },
            snapshot: { kind: "projects", projects: { projects: [] } },
          },
        }),
        {
          jsonrpc: "2.0",
          id: "server-request-1",
          scope: { kind: "task", taskId: "task-1" },
          method: QUESTION_REQUEST,
          params: {
            message: "Which environment?",
            fields: [{
              kind: "singleSelect",
              key: "environment",
              title: "Environment",
              required: true,
              options: [{ value: "target", label: "Target" }],
            }],
          },
        },
      ],
    ]);
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));
    const requests: unknown[] = [];

    connection.serverRequests((request) => requests.push(request));
    await connection.initialize(initializeParams());
    await connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } });

    expect(requests).toEqual([
      expect.objectContaining({
        requestId: "server-request-1",
        method: QUESTION_REQUEST,
        params: expect.objectContaining({ message: "Which environment?" }),
      }),
    ]);
  });

  it("rejects unknown backend-initiated server request methods", async () => {
    const fetch = fetchSequence([
      [response("local-http-request-1", { result: initializeResult() })],
      [
        response("local-http-request-2", {
          result: {
            cursor: "cursor-2",
            scope: { kind: "projects" },
            snapshot: { kind: "projects", projects: { projects: [] } },
          },
        }),
        {
          jsonrpc: "2.0",
          id: "server-request-1",
          method: "unknown/request",
          params: {},
        },
      ],
    ]);
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));

    await connection.initialize(initializeParams());
    await expect(connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } }))
      .rejects.toThrow("Unsupported App Server server request method: unknown/request");
  });

  it("rejects malformed permission server request params before notifying listeners", async () => {
    const fetch = fetchSequence([
      [response("local-http-request-1", { result: initializeResult() })],
      [
        response("local-http-request-2", {
          result: {
            cursor: "cursor-2",
            scope: { kind: "projects" },
            snapshot: { kind: "projects", projects: { projects: [] } },
          },
        }),
        {
          jsonrpc: "2.0",
          id: "server-request-1",
          scope: { kind: "task", taskId: "task-1" },
          method: PERMISSION_REQUEST,
          params: { title: "Allow?" },
        },
      ],
    ]);
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));
    const listener = vi.fn();

    connection.serverRequests(listener);
    await connection.initialize(initializeParams());
    await expect(connection.request(STATE_SUBSCRIBE, { scope: { kind: "projects" } }))
      .rejects.toThrow("permission/request params.toolCall must be an object");
    expect(listener).not.toHaveBeenCalled();
  });

  it("rejects protocol error envelopes", async () => {
    const fetch = fetchReturning([
      {
        jsonrpc: "2.0",
        id: "local-http-request-1",
        error: {
          error: { code: "validationFailed", message: "bad params", recoverable: true },
        },
      },
    ]);
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));

    await expect(connection.initialize(initializeParams())).rejects.toBeInstanceOf(AppServerProtocolError);
  });

  it("rejects non-2xx responses with HTTP status copy before protocol parsing", async () => {
    const fetch = fetchFailure(500, "internal failure");
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));

    await expect(connection.initialize(initializeParams()))
      .rejects.toThrow("App Server request failed with HTTP 500: internal failure");
  });

  it("sends server-request responses over the same LocalHttp transport", async () => {
    const fetch = fetchReturning([]);
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));

    await connection.respond("server-request-1" as RequestId, { optionId: "allow-once" });

    expect(JSON.parse(String(fetch.mock.calls[0]?.[1].body))).toEqual({
      jsonrpc: "2.0",
      id: "server-request-1",
      result: { optionId: "allow-once" },
    });
  });

  it("rejects server-request responses when the App Server returns an error for that request id", async () => {
    const fetch = fetchReturning([
      {
        jsonrpc: "2.0",
        id: "server-request-1",
        error: {
          error: {
            code: "requestAlreadyResolved",
            message: "Permission request is no longer answerable.",
            recoverable: false,
          },
        },
      },
    ]);
    const connection = createLocalHttpBackendConnection(connectionOptions(fetch));

    await expect(connection.respond("server-request-1" as RequestId, { optionId: "allow-once" }))
      .rejects.toThrow("Permission request is no longer answerable.");
  });

  it("sends heartbeat after initialize and stops on close", async () => {
    vi.useFakeTimers();
    try {
      const fetch = fetchReturning([
        response("local-http-request-1", { result: initializeResult() }),
      ]);
      const connection = createLocalHttpBackendConnection({
        ...connectionOptions(fetch),
        heartbeatIntervalMs: 25,
      });

      await connection.initialize(initializeParams());
      fetch.mockClear();

      await vi.advanceTimersByTimeAsync(25);

      expect(JSON.parse(String(fetch.mock.calls[0]?.[1].body))).toEqual({
        jsonrpc: "2.0",
        id: "local-http-request-2",
        method: CLIENT_HEARTBEAT,
        params: {},
      });

      connection.close();
      fetch.mockClear();
      await vi.advanceTimersByTimeAsync(50);

      expect(fetch).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("delivers queued app events from heartbeat responses", async () => {
    vi.useFakeTimers();
    try {
      const event = appEvent();
      const fetch = fetchSequence([
        [response("local-http-request-1", { result: initializeResult() })],
        [
          response("local-http-request-2", { result: {} }),
          { jsonrpc: "2.0", method: "app/event", params: event },
        ],
      ]);
      const connection = createLocalHttpBackendConnection({
        ...connectionOptions(fetch),
        heartbeatIntervalMs: 25,
      });
      const events: AppServerEvent[] = [];

      connection.events((received) => events.push(received));
      await connection.initialize(initializeParams());
      await vi.advanceTimersByTimeAsync(25);

      expect(events).toEqual([event]);
      connection.close();
    } finally {
      vi.useRealTimers();
    }
  });
});

function connectionOptions(fetch: ReturnType<typeof fetchReturning>) {
  return {
    endpointUrl: "http://127.0.0.1:4321",
    authToken: "token-1",
    connectionId: "client-1",
    fetch,
  };
}

function fetchReturning(messages: unknown[]) {
  return vi.fn<LocalHttpFetch>(async () => fetchResponse(messages));
}

function fetchResponse(messages: unknown[]) {
  return {
    ok: true,
    status: 200,
    async text() {
      return JSON.stringify(messages);
    },
  };
}

function eventStreamResponse(events: AppServerEvent[]) {
  const bytes = new TextEncoder().encode(
    events.map((event) => `data: ${JSON.stringify({ jsonrpc: "2.0", method: "app/event", params: event })}\n\n`).join(""),
  );
  let delivered = false;
  return {
    ok: true,
    status: 200,
    async text() {
      return "";
    },
    body: {
      getReader() {
        return {
          async read() {
            if (!delivered) {
              delivered = true;
              return { done: false, value: bytes };
            }
            return new Promise<never>(() => undefined);
          },
          async cancel() {},
        };
      },
    },
  };
}

function closedEventStreamResponse() {
  return {
    ok: true,
    status: 200,
    async text() {
      return "";
    },
    body: {
      getReader() {
        return {
          async read() {
            return { done: true };
          },
          async cancel() {},
        };
      },
    },
  };
}

function fetchFailure(status: number, text: string) {
  return vi.fn<LocalHttpFetch>(async () => ({
    ok: false,
    status,
    async text() {
      return text;
    },
  }));
}

function fetchSequence(batches: unknown[][]) {
  let nextBatch = 0;
  return vi.fn<LocalHttpFetch>(async (_input, init) => {
    if (init.headers.Accept === "text/event-stream") return fetchResponse([]);
    const messages = batches[nextBatch] ?? [];
    nextBatch += 1;
    return {
      ok: true,
      status: 200,
      async text() {
        return JSON.stringify(messages);
      },
    };
  });
}

function response(id: string, envelope: unknown) {
  return { jsonrpc: "2.0", id, result: envelope };
}

function protocolError(id: string, code: string, message: string) {
  return {
    jsonrpc: "2.0",
    id,
    error: {
      error: {
        code,
        message,
        recoverable: true,
      },
    },
  };
}

function initializeParams(): InitializeParams {
  return {
    clientInstanceId: "client-1" as ClientInstanceId,
    shell: { kind: "web" },
    requestedSurface: { kind: "home" },
  };
}

function initializeResult(serverId = "server-1", stateRootId = "root-1"): InitializeResult {
  return {
    snapshot: {
      cursor: "cursor-1",
      server: { serverId, protocolVersion: { major: 1, minor: 0 } },
      stateRoot: { stateRootId },
      client: {
        clientInstanceId: "client-1",
        shellKind: "web",
        surface: { kind: "home" },
      },
    },
  } as InitializeResult;
}

function appEvent(): AppServerEvent {
  return {
    subscription: { kind: "projects" },
    previousCursor: "cursor-1",
    cursor: "cursor-2",
    scope: { kind: "stateRoot", stateRootId: "root-1" },
    payload: { kind: "projectCollectionUpdated", projects: { projects: [] } },
  } as unknown as AppServerEvent;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}
