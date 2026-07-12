import {
  type BackendConnection,
  type BackendEventListener,
  type BackendServerRequestListener,
  type BackendStateResetListener,
  type BackendUnsubscribe,
} from "./backendConnection.js";
import type {
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
import { CLIENT_HEARTBEAT, CLIENT_INITIALIZE } from "./generated/protocol.js";
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

function createHttpBackendConnection(
  options: HttpBackendConnectionOptions,
): BackendConnection {
  const fetchImpl = options.fetch ?? globalThis.fetch?.bind(globalThis);
  if (!fetchImpl) throw new Error("LocalHttp BackendConnection requires fetch");

  const events = new Set<BackendEventListener>();
  const stateResetListeners = new Set<BackendStateResetListener>();
  const serverRequests = new Set<BackendServerRequestListener>();
  let nextId = 1;
  let initialized = false;
  let closed = false;
  let initializeResult: InitializeResult | undefined;
  let lastInitializeMeta: RequestMeta | undefined;
  let lastInitializeParams: InitializeParams | undefined;
  let initializePromise: Promise<InitializeResult> | undefined;
  let reinitializePromise: Promise<void> | undefined;
  let heartbeatTimer: ReturnType<typeof setInterval> | undefined;
  let eventStreamAbort: AbortController | undefined;
  let eventStreamReader: LocalHttpStreamReader | undefined;
  let eventStreamRetryTimer: ReturnType<typeof setTimeout> | undefined;
  let eventStreamContinuityLost = false;

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
      if (!initialized) {
        if (!lastInitializeParams) {
          return Promise.reject(new Error("Backend connection is not initialized"));
        }
        // A failed restart attempt must not make the connection terminal. The
        // caller's normal retry is also the next initialization attempt, and
        // concurrent callers share the same reinitialize promise.
        return reinitializeConnection().then(() => sendRequestWithInitializedRetry(method, params, meta));
      }
      return sendRequestWithInitializedRetry(method, params, meta);
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
      lastInitializeMeta = undefined;
      lastInitializeParams = undefined;
      initializePromise = undefined;
      reinitializePromise = undefined;
      stopHeartbeat();
      stopEventStream();
      events.clear();
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

  async function sendRequestWithInitializedRetry<M extends ProtocolMethod>(
    method: M,
    params: RequestParamsByMethod[M],
    meta?: RequestMeta,
  ): Promise<ResponseResultByMethod[M]> {
    try {
      return await sendRequest(method, params, meta);
    } catch (error) {
      if (!isNotInitializedError(error) || !lastInitializeParams) throw error;
      await reinitializeConnection();
      return sendRequest(method, params, meta);
    }
  }

  async function reinitializeConnection() {
    if (!lastInitializeParams) throw new Error("Backend connection has no initialization context");
    if (reinitializePromise) return reinitializePromise;
    const params = lastInitializeParams;
    const meta = lastInitializeMeta;
    reinitializePromise = (async () => {
      // Initialization owns the server-side subscription registry. Replacing it
      // always invalidates every watched snapshot, regardless of which request
      // first discovered the restart.
      eventStreamContinuityLost = true;
      // The event stream starts from initialize's success callback, so a very
      // fast 409 can race the promise's final cleanup. Let that generation settle
      // before replacing it.
      if (initializePromise) await initializePromise;
      initialized = false;
      initializeResult = undefined;
      await connection.initialize(params, meta);
      notifyStateResetIfNeeded();
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
      void sendRequestWithInitializedRetry(CLIENT_HEARTBEAT, {}, undefined).catch(() => {
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
    void consumeEventStream(eventStreamAbort.signal);
  }

  function stopEventStream() {
    if (eventStreamRetryTimer) clearTimeout(eventStreamRetryTimer);
    eventStreamRetryTimer = undefined;
    eventStreamAbort?.abort();
    eventStreamAbort = undefined;
    void eventStreamReader?.cancel().catch(() => undefined);
    eventStreamReader = undefined;
  }

  async function consumeEventStream(signal: AbortSignal) {
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
        await reinitializeConnection();
        return;
      }
      if (!response.ok || !response.body) return;
      notifyStateResetIfNeeded();
      reader = response.body.getReader();
      eventStreamReader = reader;
      const decoder = new TextDecoder();
      let buffered = "";
      while (!closed && !signal.aborted) {
        const { done, value } = await reader.read();
        if (done) break;
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
        eventStreamRetryTimer = setTimeout(() => startEventStream(), EVENT_STREAM_RETRY_MS);
      }
    }
  }

  function notifyStateResetIfNeeded() {
    if (!eventStreamContinuityLost) return;
    eventStreamContinuityLost = false;
    for (const listener of stateResetListeners) listener();
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
