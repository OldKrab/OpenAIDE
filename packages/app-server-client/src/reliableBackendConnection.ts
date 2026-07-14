import type {
  BackendConnection,
  BackendEventListener,
  BackendUnsubscribe,
} from "./backendConnection.js";
import {
  CLIENT_HEARTBEAT,
  CLIENT_INITIALIZE,
  type AppServerEvent,
  type InitializeParams,
  type InitializeResult,
  type ProtocolMethod,
  type RequestMeta,
  type RequestParamsByMethod,
  type ResponseEnvelope,
  type ResponseResultByMethod,
  type ServerRequestMethod,
  type ServerRequestParamsByMethod,
  type ServerRequestResponseResultByMethod,
} from "./generated/protocol.js";
import { AppServerProtocolError, errorEnvelopeFromUnknown } from "./protocolError.js";
import {
  createRpcPeer,
  RpcResponseError,
  type RpcMessageChannel,
  type RpcNotificationMap,
  type RpcRequestMap,
} from "./rpcPeer.js";
import {
  createReliableHttpMessageChannel,
  isReliableHttpSessionExpired,
  type ReliableHttpFetch,
  type ReliableHttpMessageChannel,
} from "./reliableHttpChannel.js";

type ClientRequests = RpcRequestMap & {
  [M in ProtocolMethod]: {
    params: RequestParamsByMethod[M];
    result: ResponseEnvelope<ResponseResultByMethod[M]>;
  };
};

type ServerRequests = RpcRequestMap & {
  [M in ServerRequestMethod]: {
    params: ServerRequestParamsByMethod[M];
    result: ServerRequestResponseResultByMethod[M];
  };
};

type ServerNotifications = RpcNotificationMap & {
  "app/event": { params: AppServerEvent };
};

export type ReliableBackendConnectionOptions = {
  channel: RpcMessageChannel & { close?(): void };
  heartbeatIntervalMs?: number;
};

type InternalReliableBackendConnectionOptions = ReliableBackendConnectionOptions & {
  onRequestError?: (error: unknown, method: ProtocolMethod) => void;
};

export type ReliableLocalHttpBackendConnectionOptions = {
  endpointUrl: string;
  authToken: string;
  connectionId: string;
  fetch?: ReliableHttpFetch;
  heartbeatIntervalMs?: number;
  retryDelayMs?: number;
};

export type ReliableWebProxyBackendConnectionOptions = Omit<
  ReliableLocalHttpBackendConnectionOptions,
  "authToken"
>;

export function createReliableLocalHttpBackendConnection(
  options: ReliableLocalHttpBackendConnectionOptions,
) {
  return createReliableHttpBackendConnection(options);
}

export function createReliableWebProxyBackendConnection(
  options: ReliableWebProxyBackendConnectionOptions,
) {
  return createReliableHttpBackendConnection(options);
}

function createReliableHttpBackendConnection(
  options: ReliableLocalHttpBackendConnectionOptions | ReliableWebProxyBackendConnectionOptions,
): BackendConnection {
  const eventListeners = new Set<BackendEventListener>();
  const requestRegistrations = new Set<{
    bind(connection: BackendConnection): void;
    dispose(): void;
  }>();
  const generations = new Set<HttpConnectionGeneration>();
  let active = createGeneration();
  let initializedServerId: string | undefined;
  let initializeParams: InitializeParams | undefined;
  let initializeMeta: RequestMeta | undefined;
  let initializePromise: Promise<InitializeResult> | undefined;
  let recoveryPromise: Promise<InitializeResult> | undefined;
  let terminalError: unknown;
  let closed = false;
  bindGeneration(active);

  return {
    initialize(params, meta) {
      if (initializePromise) return initializePromise;
      initializeParams = params;
      initializeMeta = meta;
      const generation = active;
      initializePromise = initializeGeneration(generation).catch(async (error) => {
        // Expiry can race the first initialization response. In that case the
        // caller observes the replacement initialization, not a stale failure.
        if (recoveryPromise) return recoveryPromise;
        throw error;
      });
      return initializePromise;
    },
    async request(method, params, meta) {
      if (closed) throw new Error("Backend connection is closed");
      if (terminalError) throw terminalError;
      if (!initializeParams) throw new Error("Backend connection is not initialized");
      // Requests created after expiry wait for the fresh initialized session.
      // Ambiguous requests already sent through a lost transport are never replayed.
      if (recoveryPromise) await recoveryPromise;
      if (terminalError) throw terminalError;
      const generation = active;
      try {
        return await generation.connection.request(method, params, meta);
      } catch (error) {
        const recovery = recoveryPromise;
        if (!isNotInitialized(error)) throw error;
        // notInitialized is an authoritative pre-dispatch rejection. Unlike an
        // HTTP 410, it proves that even a non-idempotent request did not run.
        if (recovery) await recovery;
        if (terminalError) throw terminalError;
        if (active === generation) throw error;
        return active.connection.request(method, params, meta);
      }
    },
    handleRequest(method, handler) {
      let unsubscribe = active.connection.handleRequest(method, handler);
      const registration = {
        bind(connection: BackendConnection) {
          unsubscribe();
          unsubscribe = connection.handleRequest(method, handler);
        },
        dispose() {
          unsubscribe();
          requestRegistrations.delete(registration);
        },
      };
      requestRegistrations.add(registration);
      return registration.dispose;
    },
    handleNotification(_method, handler) {
      eventListeners.add(handler);
      return () => eventListeners.delete(handler);
    },
    close() {
      if (closed) return;
      closed = true;
      for (const generation of generations) closeGeneration(generation);
      generations.clear();
      requestRegistrations.clear();
      eventListeners.clear();
    },
  };

  function createGeneration(): HttpConnectionGeneration {
    const channel = createReliableHttpMessageChannel({
      endpointUrl: options.endpointUrl,
      connectionId: options.connectionId,
      ...("authToken" in options ? { authToken: options.authToken } : {}),
      ...(options.fetch ? { fetch: options.fetch } : {}),
      ...(options.retryDelayMs === undefined ? {} : { retryDelayMs: options.retryDelayMs }),
    });
    let generation: HttpConnectionGeneration;
    const connection = createInternalReliableBackendConnection({
      channel,
      ...(options.heartbeatIntervalMs === undefined
        ? {}
        : { heartbeatIntervalMs: options.heartbeatIntervalMs }),
      onRequestError(error, method) {
        if (method !== CLIENT_INITIALIZE) handleGenerationRequestError(generation, error);
      },
    });
    generation = { channel, connection };
    generations.add(generation);
    generation.unsubscribeError = channel.subscribeErrors?.((error) => {
      handleGenerationError(generation, error);
    });
    return generation;
  }

  function bindGeneration(generation: HttpConnectionGeneration) {
    generation.unsubscribeEvent = generation.connection.handleNotification("app/event", (event) => {
      for (const listener of eventListeners) listener(event);
    });
    for (const registration of requestRegistrations) registration.bind(generation.connection);
  }

  async function initializeGeneration(generation: HttpConnectionGeneration) {
    const params = initializeParams;
    if (!params) throw new Error("Backend connection is not initialized");
    const identity = await generation.channel.ready();
    if (initializedServerId && identity.serverId !== initializedServerId) {
      throw new Error("App Server instance changed while replacing an expired HTTP session");
    }
    initializedServerId = identity.serverId;
    return generation.connection.initialize(params, initializeMeta);
  }

  function handleGenerationError(generation: HttpConnectionGeneration, error: unknown) {
    if (closed || generation !== active) return;
    if (!isReliableHttpSessionExpired(error) || !initializeParams) {
      terminalError = error;
      return;
    }
    beginRecovery(generation, "HTTP RPC session expired");
  }

  function handleGenerationRequestError(
    generation: HttpConnectionGeneration,
    error: unknown,
  ) {
    if (
      closed
      || generation !== active
      || !(error instanceof AppServerProtocolError)
      || error.protocolError.code !== "notInitialized"
      || !initializeParams
    ) return;
    beginRecovery(generation, "App Server client liveness expired");
  }

  function beginRecovery(generation: HttpConnectionGeneration, reason: string) {
    if (recoveryPromise) return;
    console.info(`[OpenAIDE] ${reason}; reinitializing the connection`);
    const attempt = recoverGeneration(generation);
    recoveryPromise = attempt;
    void attempt.then(
      () => {
        if (recoveryPromise === attempt) recoveryPromise = undefined;
        console.info("[OpenAIDE] App Server connection reinitialized after expiry");
      },
      (recoveryError) => {
        if (recoveryPromise === attempt) recoveryPromise = undefined;
        terminalError = recoveryError;
        console.error("[OpenAIDE] Failed to restore App Server connection after expiry", recoveryError);
      },
    );
  }

  async function recoverGeneration(previous: HttpConnectionGeneration) {
    const replacement = createGeneration();
    try {
      const identity = await replacement.channel.ready();
      if (initializedServerId && identity.serverId !== initializedServerId) {
        throw new Error("App Server instance changed while replacing an expired HTTP session");
      }
      active = replacement;
      bindGeneration(replacement);
      closeGeneration(previous);
      return await initializeGeneration(replacement);
    } catch (error) {
      closeGeneration(replacement);
      throw error;
    }
  }

  function closeGeneration(generation: HttpConnectionGeneration) {
    if (!generations.delete(generation)) return;
    generation.unsubscribeError?.();
    generation.unsubscribeEvent?.();
    generation.connection.close();
    generation.channel.close();
  }
}

type HttpConnectionGeneration = {
  channel: ReliableHttpMessageChannel;
  connection: BackendConnection;
  unsubscribeError?: BackendUnsubscribe;
  unsubscribeEvent?: BackendUnsubscribe;
};

/** Adapts the generated App Server contract onto the transport-independent peer. */
export function createReliableBackendConnection(
  options: ReliableBackendConnectionOptions,
): BackendConnection {
  return createInternalReliableBackendConnection(options);
}

function createInternalReliableBackendConnection(
  options: InternalReliableBackendConnectionOptions,
): BackendConnection {
  const peer = createRpcPeer<
    ClientRequests,
    RpcNotificationMap,
    ServerRequests,
    ServerNotifications
  >(options.channel);
  const eventListeners = new Set<BackendEventListener>();
  let initialized = false;
  let initializePromise: Promise<InitializeResult> | undefined;
  let heartbeat: ReturnType<typeof setInterval> | undefined;

  // RpcPeer owns the single protocol handler. Backend consumers are independent
  // projections of that notification stream and therefore need local multicast.
  peer.handleNotification("app/event", (event) => {
    for (const listener of eventListeners) listener(event);
  });

  const connection: BackendConnection = {
    initialize(params: InitializeParams, meta?: RequestMeta) {
      if (initializePromise) return initializePromise;
      initializePromise = sendRequest(CLIENT_INITIALIZE, params, meta).then((result) => {
        initialized = true;
        startHeartbeat();
        return result;
      });
      return initializePromise;
    },
    request(method, params, meta) {
      if (!initialized) return Promise.reject(new Error("Backend connection is not initialized"));
      return sendRequest(method, params, meta);
    },
    handleRequest(method, handler) {
      return peer.handleRequest(method, (params, context) => handler(params as never, {
        requestId: String(context.requestId) as import("./generated/protocol.js").RequestId,
        scope: context.scope,
        signal: context.signal,
      })) as BackendUnsubscribe;
    },
    handleNotification(_method, handler) {
      eventListeners.add(handler);
      return () => eventListeners.delete(handler);
    },
    close() {
      if (heartbeat) clearInterval(heartbeat);
      heartbeat = undefined;
      initialized = false;
      peer.close();
      options.channel.close?.();
      eventListeners.clear();
    },
  };
  return connection;

  async function sendRequest<M extends ProtocolMethod>(
    method: M,
    params: RequestParamsByMethod[M],
    meta?: RequestMeta,
  ): Promise<ResponseResultByMethod[M]> {
    try {
      const response = await peer.request(method, params, meta === undefined ? undefined : {
        meta,
      }) as unknown as ResponseEnvelope<
        ResponseResultByMethod[M]
      >;
      return response.result;
    } catch (error) {
      let requestError = error;
      if (error instanceof RpcResponseError) {
        const envelope = errorEnvelopeFromUnknown(error.responseError);
        if (envelope) requestError = new AppServerProtocolError(envelope);
      }
      options.onRequestError?.(requestError, method);
      throw requestError;
    }
  }

  function startHeartbeat() {
    if (heartbeat) clearInterval(heartbeat);
    heartbeat = setInterval(() => {
      if (!initialized) return;
      void sendRequest(CLIENT_HEARTBEAT, {}).catch(() => undefined);
    }, options.heartbeatIntervalMs ?? 5_000);
  }
}

function isNotInitialized(error: unknown) {
  return error instanceof AppServerProtocolError
    && error.protocolError.code === "notInitialized";
}
