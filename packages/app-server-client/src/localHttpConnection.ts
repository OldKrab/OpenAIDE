import {
  type BackendConnection,
  type BackendEventListener,
  type BackendEventStreamDisconnectListener,
  type BackendServerRequestListener,
  type BackendStateReset,
  type BackendStateResetListener,
  type BackendUnsubscribe,
} from "./backendConnection.js";
import { BackendReplicaChangedError } from "./backendReplicaChangedError.js";
import type {
  ClientCapabilitiesChangedParams,
  InitializeParams,
  InitializeResult,
  ProtocolMethod,
  RequestId,
  RequestMeta,
  RequestParamsByMethod,
  ResponseResultByMethod,
  ServerRequestMethod,
  ServerRequestResponseResultByMethod,
} from "./generated/protocol.js";
import {
  CLIENT_CAPABILITIES_CHANGED,
  CLIENT_HEARTBEAT,
  CLIENT_INITIALIZE,
} from "./generated/protocol.js";
import {
  parseLocalHttpWireMessages,
  responseResultForId,
  type JsonRpcId,
  type LocalHttpWireMessage,
} from "./localHttpWire.js";
import { AppServerProtocolError, protocolErrorFromUnknown } from "./protocolError.js";

export type LocalHttpConnectionInfo = {
  endpointUrl: string;
  authToken: string;
  connectionId: string;
};

export type WebProxyConnectionInfo = {
  endpointUrl: string;
  connectionId: string;
};

export type LocalHttpBackendConnectionOptions = LocalHttpConnectionInfo & {
  fetch?: LocalHttpFetch;
  requestTimeoutMs?: number;
  heartbeatIntervalMs?: number;
};

export type WebProxyBackendConnectionOptions = WebProxyConnectionInfo & {
  fetch?: LocalHttpFetch;
  requestTimeoutMs?: number;
  heartbeatIntervalMs?: number;
};

export type LocalHttpFetch = (
  input: string,
  init: {
    method: "POST";
    headers: Record<string, string>;
    body: string;
    signal?: AbortSignal;
  },
) => Promise<LocalHttpFetchResponse>;

export type LocalHttpFetchResponse = {
  ok: boolean;
  status: number;
  text(): Promise<string>;
  body?: LocalHttpResponseBody | null;
};

type LocalHttpResponseBody = {
  getReader(): LocalHttpStreamReader;
};

type LocalHttpStreamReader = {
  read(): Promise<{ done: boolean; value?: Uint8Array }>;
  cancel(): Promise<void>;
};

export function createLocalHttpBackendConnection(
  options: LocalHttpBackendConnectionOptions,
): BackendConnection {
  return createHttpBackendConnection({ ...options, authToken: options.authToken });
}

export function createWebProxyBackendConnection(
  options: WebProxyBackendConnectionOptions,
): BackendConnection {
  return createHttpBackendConnection({ ...options, authToken: undefined });
}

type HttpBackendConnectionOptions = (LocalHttpBackendConnectionOptions | WebProxyBackendConnectionOptions) & {
  authToken?: string;
};

const DEFAULT_REQUEST_TIMEOUT_MS = 120_000;
const EVENT_STREAM_RETRY_MS = 500;
const MAX_EVENT_STREAM_RETRY_MS = 5_000;

function createHttpBackendConnection(
  options: HttpBackendConnectionOptions,
): BackendConnection {
  const fetchImpl = options.fetch ?? globalThis.fetch?.bind(globalThis);
  if (!fetchImpl) throw new Error("LocalHttp BackendConnection requires fetch");

  const events = new Set<BackendEventListener>();
  const eventStreamDisconnectListeners = new Set<BackendEventStreamDisconnectListener>();
  const stateResetListeners = new Set<BackendStateResetListener>();
  const serverRequests = new Set<BackendServerRequestListener>();
  let nextId = 1;
  let initialized = false;
  let closed = false;
  let initializeResult: InitializeResult | undefined;
  let lastSuccessfulReplica: BackendStateReset | undefined;
  let replicaGeneration = 0;
  let lastInitializeMeta: RequestMeta | undefined;
  let lastInitializeParams: InitializeParams | undefined;
  let initializePromise: Promise<InitializeResult> | undefined;
  let reinitializePromise: Promise<void> | undefined;
  let heartbeatTimer: ReturnType<typeof setInterval> | undefined;
  let eventStreamAbort: AbortController | undefined;
  let eventStreamReader: LocalHttpStreamReader | undefined;
  let eventStreamRetryTimer: ReturnType<typeof setTimeout> | undefined;
  let eventStreamRetryDelay = EVENT_STREAM_RETRY_MS;
  let eventStreamContinuityLost = false;
  let eventStreamDisconnected = false;

  const connection: BackendConnection = {
    async initialize(params: InitializeParams, meta?: RequestMeta) {
      if (closed) throw new Error("Backend connection closed");
      if (initializeResult) return initializeResult;
      if (initializePromise) return initializePromise;
      lastInitializeParams = params;
      lastInitializeMeta = meta;

      initializePromise = sendRequest(CLIENT_INITIALIZE, params, meta)
        .then((result) => {
          if (closed) throw new Error("Backend connection closed");
          initialized = true;
          initializeResult = result;
          lastSuccessfulReplica = replicaIdentity(result);
          startHeartbeat();
          startEventStream();
          return result;
        })
        .finally(() => {
          initializePromise = undefined;
        });
      return initializePromise;
    },
    request<M extends ProtocolMethod>(
      method: M,
      params: RequestParamsByMethod[M],
      meta?: RequestMeta,
    ) {
      if (closed) return Promise.reject(new Error("Backend connection closed"));
      // Capability declarations are part of initialization context. Remember
      // them before waiting for a replacement process so recovery never starts
      // with workspace facts that this caller has already superseded.
      rememberInitializationContextUpdate(method, params);
      const originGeneration = replicaGeneration;
      const previousReplica = lastSuccessfulReplica;
      if (!initialized || reinitializePromise) {
        if (!lastInitializeParams) {
          return Promise.reject(new Error("Backend connection is not initialized"));
        }
        return reinitializeThenRejectRequest(method, previousReplica, originGeneration);
      }
      return sendRequestWithinReplica(method, params, meta, originGeneration, previousReplica);
    },
    respond<M extends ServerRequestMethod>(
      requestId: RequestId,
      result: ServerRequestResponseResultByMethod[M],
    ) {
      if (closed) return Promise.reject(new Error("Backend connection closed"));
      return sendJsonRpc(requestId, { jsonrpc: "2.0", id: requestId, result }).then((messages) => {
        throwResponseErrorForId(messages, requestId);
        processSideEffects(messages);
      });
    },
    events(listener: BackendEventListener): BackendUnsubscribe {
      events.add(listener);
      return () => events.delete(listener);
    },
    eventStreamDisconnects(listener: BackendEventStreamDisconnectListener): BackendUnsubscribe {
      eventStreamDisconnectListeners.add(listener);
      if (eventStreamDisconnected) listener();
      return () => eventStreamDisconnectListeners.delete(listener);
    },
    stateResets(listener: BackendStateResetListener): BackendUnsubscribe {
      stateResetListeners.add(listener);
      return () => stateResetListeners.delete(listener);
    },
    serverRequests(listener: BackendServerRequestListener): BackendUnsubscribe {
      serverRequests.add(listener);
      return () => serverRequests.delete(listener);
    },
    close() {
      closed = true;
      initialized = false;
      initializeResult = undefined;
      lastSuccessfulReplica = undefined;
      lastInitializeMeta = undefined;
      lastInitializeParams = undefined;
      initializePromise = undefined;
      reinitializePromise = undefined;
      stopHeartbeat();
      stopEventStream();
      events.clear();
      eventStreamDisconnectListeners.clear();
      stateResetListeners.clear();
      serverRequests.clear();
    },
  };

  return connection;

  async function sendRequest<M extends ProtocolMethod>(
    method: M,
    params: RequestParamsByMethod[M],
    meta?: RequestMeta,
  ): Promise<ResponseResultByMethod[M]> {
    const id = createRequestId();
    const messages = await sendJsonRpc(id, {
      jsonrpc: "2.0",
      id,
      method,
      params,
      ...(meta ? { meta } : {}),
    });
    processSideEffects(messages);
    return responseResultForId(messages, id) as ResponseResultByMethod[M];
  }

  async function sendRequestWithinReplica<M extends ProtocolMethod>(
    method: M,
    params: RequestParamsByMethod[M],
    meta?: RequestMeta,
    originGeneration = replicaGeneration,
    previousReplica = lastSuccessfulReplica,
  ): Promise<ResponseResultByMethod[M]> {
    let result: ResponseResultByMethod[M];
    try {
      result = await sendRequest(method, params, meta);
    } catch (error) {
      if (!isNotInitializedError(error) || !lastInitializeParams) throw error;
      await reinitializeConnection(originGeneration);
      throw replicaChangedError(method, previousReplica);
    }
    if (originGeneration !== replicaGeneration) {
      if (reinitializePromise) await reinitializePromise;
      throw replicaChangedError(method, previousReplica);
    }
    return result;
  }

  async function reinitializeThenRejectRequest<M extends ProtocolMethod>(
    method: M,
    previousReplica: BackendStateReset | undefined,
    originGeneration: number,
  ): Promise<ResponseResultByMethod[M]> {
    await reinitializeConnection(originGeneration);
    throw replicaChangedError(method, previousReplica);
  }

  function replicaChangedError(
    method: ProtocolMethod,
    previousReplica: BackendStateReset | undefined,
  ) {
    if (!lastSuccessfulReplica) {
      throw new Error("App Server reinitialized without a replica identity");
    }
    return new BackendReplicaChangedError(method, previousReplica, lastSuccessfulReplica);
  }

  function rememberInitializationContextUpdate<M extends ProtocolMethod>(
    method: M,
    params: RequestParamsByMethod[M],
  ) {
    if (method !== CLIENT_CAPABILITIES_CHANGED || !lastInitializeParams) return;
    const update = params as ClientCapabilitiesChangedParams;
    const next = { ...lastInitializeParams };
    if (update.capabilities !== undefined && update.capabilities !== null) {
      next.capabilities = update.capabilities;
    }
    if (update.workspaceRoots !== undefined && update.workspaceRoots !== null) {
      next.workspaceRoots = update.workspaceRoots.map(({ path }) => ({ path }));
    }
    lastInitializeParams = next;
  }

  async function reinitializeConnection(originGeneration = replicaGeneration) {
    if (!lastInitializeParams) throw new Error("Backend connection has no initialization context");
    if (reinitializePromise) return reinitializePromise;
    // A response from an older generation may arrive after another request has
    // already completed recovery. It belongs to that same lost replica and must
    // not cause a second initialization cycle.
    if (originGeneration !== replicaGeneration) return;
    const params = lastInitializeParams;
    const meta = lastInitializeMeta;
    replicaGeneration += 1;
    reinitializePromise = (async () => {
      // Initialization owns the server-side subscription registry. Replacing it
      // always invalidates every watched snapshot, regardless of which request
      // first discovered the restart.
      eventStreamContinuityLost = true;
      notifyEventStreamDisconnected();
      // The event stream starts from initialize's success callback, so a very
      // fast 409 can race the promise's final cleanup. Let that generation settle
      // before replacing it.
      if (initializePromise) await initializePromise;
      initialized = false;
      initializeResult = undefined;
      await connection.initialize(params, meta);
    })().finally(() => {
      reinitializePromise = undefined;
    });
    return reinitializePromise;
  }

  async function sendJsonRpc(id: JsonRpcId, body: unknown): Promise<LocalHttpWireMessage[]> {
    try {
      const response = await fetchWithTimeout(fetchImpl, options, JSON.stringify(body));
      const text = await response.text();
      if (!response.ok) {
        throw new Error(httpFailureMessage(response.status, text));
      }
      return parseLocalHttpWireMessages(text);
    } catch (error) {
      if (closed) throw new Error("Backend connection closed");
      if (isAbortError(error)) {
        throw new Error("App Server request timed out. The Agent session may still be loading; try again.");
      }
      throw protocolErrorFromUnknown(error);
    }
  }

  function processSideEffects(messages: LocalHttpWireMessage[]) {
    for (const message of messages) {
      if (message.kind === "event") {
        for (const listener of events) listener(message.event);
      }
      if (message.kind === "serverRequest") {
        for (const listener of serverRequests) listener(message.request);
      }
    }
  }

  function throwResponseErrorForId(messages: LocalHttpWireMessage[], id: JsonRpcId) {
    if (!messages.some((message) => message.kind === "response" && message.id === id)) return;
    responseResultForId(messages, id);
  }

  function createRequestId(): string {
    const id = `local-http-request-${nextId}`;
    nextId += 1;
    return id;
  }

  function startHeartbeat() {
    stopHeartbeat();
    const interval = options.heartbeatIntervalMs ?? 5_000;
    heartbeatTimer = setInterval(() => {
      if (closed || !initialized) return;
      void sendRequestWithinReplica(CLIENT_HEARTBEAT, {}, undefined).catch(() => {
        // Normal requests surface transport failures. Heartbeat failure is only
        // a liveness hint for the App Server, so keep UI state owned by callers.
      });
    }, interval);
  }

  function stopHeartbeat() {
    if (heartbeatTimer !== undefined) {
      clearInterval(heartbeatTimer);
      heartbeatTimer = undefined;
    }
  }

  function startEventStream() {
    stopEventStream();
    eventStreamAbort = new AbortController();
    void consumeEventStream(eventStreamAbort.signal, replicaGeneration);
  }

  function stopEventStream() {
    if (eventStreamRetryTimer) clearTimeout(eventStreamRetryTimer);
    eventStreamRetryTimer = undefined;
    eventStreamAbort?.abort();
    eventStreamAbort = undefined;
    void eventStreamReader?.cancel().catch(() => undefined);
    eventStreamReader = undefined;
  }

  async function consumeEventStream(signal: AbortSignal, originGeneration: number) {
    let reader: LocalHttpStreamReader | undefined;
    try {
      const response = await fetchImpl(options.endpointUrl, {
        method: "POST",
        headers: requestHeaders(options, { Accept: "text/event-stream" }),
        body: "",
        signal,
      });
      if (response.status === 409 && lastInitializeParams) {
        // A restarted App Server no longer knows this connection or its scopes.
        // Reinitialize first; the replacement stream will then invalidate all
        // watched state so callers can establish fresh snapshots.
        eventStreamContinuityLost = true;
        await reinitializeConnection(originGeneration);
        return;
      }
      if (!response.ok || !response.body) return;
      eventStreamDisconnected = false;
      notifyStateResetIfNeeded();
      reader = response.body.getReader();
      eventStreamReader = reader;
      const decoder = new TextDecoder();
      let buffered = "";
      while (!closed && !signal.aborted) {
        const { done, value } = await reader.read();
        if (done) break;
        eventStreamRetryDelay = EVENT_STREAM_RETRY_MS;
        buffered += decoder.decode(value, { stream: true });
        const frames = buffered.split(/\r?\n\r?\n/);
        buffered = frames.pop() ?? "";
        for (const frame of frames) processEventStreamFrame(frame);
      }
    } catch (error) {
      if (!signal.aborted && !closed) {
        // Heartbeats continue to recover queued events when streaming is unavailable.
      }
    } finally {
      if (eventStreamReader === reader) eventStreamReader = undefined;
      if (!signal.aborted && !closed && eventStreamAbort?.signal === signal) {
        // The server drains deliveries before writing them, so any broken stream
        // invalidates all subscription cursors even when no gap event follows.
        eventStreamContinuityLost = true;
        notifyEventStreamDisconnected();
        // A reset means a replacement stream is ready for fresh baselines. Emitting
        // it while disconnected makes every scope retry independently and can
        // install snapshots that have no live event stream behind them.
        const retryDelay = eventStreamRetryDelay;
        eventStreamRetryDelay = Math.min(eventStreamRetryDelay * 2, MAX_EVENT_STREAM_RETRY_MS);
        eventStreamRetryTimer = setTimeout(() => startEventStream(), retryDelay);
      }
    }
  }

  function notifyStateResetIfNeeded() {
    if (!eventStreamContinuityLost) return;
    const snapshot = initializeResult?.snapshot;
    // Event streams start only after initialize succeeds, so a continuity reset
    // always has an authoritative process and state-root identity.
    if (!snapshot) return;
    eventStreamContinuityLost = false;
    const reset = {
      serverId: snapshot.server.serverId,
      stateRootId: snapshot.stateRoot.stateRootId,
    };
    for (const listener of stateResetListeners) listener(reset);
  }

  function notifyEventStreamDisconnected() {
    if (eventStreamDisconnected) return;
    eventStreamDisconnected = true;
    for (const listener of eventStreamDisconnectListeners) listener();
  }

  function processEventStreamFrame(frame: string) {
    const data = frame
      .split(/\r?\n/)
      .filter((line) => line.startsWith("data:"))
      .map((line) => line.slice(5).trimStart())
      .join("\n");
    if (!data) return;
    processSideEffects(parseLocalHttpWireMessages(data));
  }
}

function httpFailureMessage(status: number, text: string) {
  const detail = text.trim().replace(/\s+/g, " ").slice(0, 180);
  return detail
    ? `App Server request failed with HTTP ${status}: ${detail}`
    : `App Server request failed with HTTP ${status}`;
}

function isAbortError(error: unknown) {
  if (!(error instanceof Error)) return false;
  return error.name === "AbortError" || error.message.includes("aborted");
}

function isNotInitializedError(error: unknown) {
  return error instanceof AppServerProtocolError && error.protocolError.code === "notInitialized";
}

function replicaIdentity(result: InitializeResult | undefined) {
  const snapshot = result?.snapshot;
  return snapshot
    ? {
      serverId: snapshot.server.serverId,
      stateRootId: snapshot.stateRoot.stateRootId,
    }
    : undefined;
}

async function fetchWithTimeout(
  fetchImpl: LocalHttpFetch,
  options: HttpBackendConnectionOptions,
  body: string,
): Promise<LocalHttpFetchResponse> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), options.requestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS);
  try {
    return await fetchImpl(options.endpointUrl, {
      method: "POST",
      headers: requestHeaders(options),
      body,
      signal: controller.signal,
    });
  } finally {
    clearTimeout(timeout);
  }
}

function requestHeaders(
  options: HttpBackendConnectionOptions,
  extra: Record<string, string> = {},
) {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    "X-OpenAIDE-Connection-Id": options.connectionId,
    ...extra,
  };
  if (options.authToken) headers.Authorization = `Bearer ${options.authToken}`;
  return headers;
}
