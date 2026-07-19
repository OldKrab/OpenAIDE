import type {
  BackendConnection,
  BackendEventListener,
  BackendGenerationInvalidation,
  BackendRecoveryBaseline,
  BackendRecoveryFailure,
  BackendUnsubscribe,
  AppServerSession,
} from "./backendConnection.js";
import { createAppServerSession } from "./appServerSession.js";
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
  isReliableHttpReplayExpired,
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
  subscribeToWake?: (wake: () => void) => () => void;
  /** Supplies a replacement endpoint when the App Shell starts a new App Server process. */
  subscribeToReplacement?: (
    replace: (endpoint: { endpointUrl: string; authToken: string }) => void,
  ) => () => void;
};

export type ReliableWebProxyBackendConnectionOptions = Omit<
  ReliableLocalHttpBackendConnectionOptions,
  "authToken" | "subscribeToReplacement"
>;

export function createReliableLocalHttpBackendConnection(
  options: ReliableLocalHttpBackendConnectionOptions,
): AppServerSession {
  return createAppServerSession(createReliableHttpBackendConnection(options));
}

export function createReliableWebProxyBackendConnection(
  options: ReliableWebProxyBackendConnectionOptions,
): AppServerSession {
  return createAppServerSession(createReliableHttpBackendConnection(options));
}

function createReliableHttpBackendConnection(
  options: ReliableLocalHttpBackendConnectionOptions | ReliableWebProxyBackendConnectionOptions,
): BackendConnection {
  const eventListeners = new Set<BackendEventListener>();
  const generationInvalidationListeners = new Set<
    (event: BackendGenerationInvalidation) => void
  >();
  const recoveryBaselineListeners = new Set<(event: BackendRecoveryBaseline) => void>();
  const recoveryFailureListeners = new Set<(event: BackendRecoveryFailure) => void>();
  const requestRegistrations = new Set<{
    bind(connection: BackendConnection): void;
    dispose(): void;
  }>();
  const generations = new Set<HttpConnectionGeneration>();
  let endpoint: { endpointUrl: string; authToken?: string } = {
    endpointUrl: options.endpointUrl,
    ...("authToken" in options ? { authToken: options.authToken } : {}),
  };
  let endpointRevision = 0;
  let active = createGeneration();
  let initializedServerId: string | undefined;
  let initializeParams: InitializeParams | undefined;
  let initializeMeta: RequestMeta | undefined;
  let initializePromise: Promise<InitializeResult> | undefined;
  let recoveryPromise: Promise<InitializeResult> | undefined;
  let recoveringGeneration: HttpConnectionGeneration | undefined;
  let terminalError: unknown;
  let closed = false;
  bindGeneration(active);
  const stopReplacement = "subscribeToReplacement" in options
    ? options.subscribeToReplacement?.(replaceEndpoint)
    : undefined;

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
    handleGenerationInvalidated(handler) {
      generationInvalidationListeners.add(handler);
      return () => generationInvalidationListeners.delete(handler);
    },
    handleRecoveryBaseline(handler) {
      recoveryBaselineListeners.add(handler);
      return () => recoveryBaselineListeners.delete(handler);
    },
    handleRecoveryFailed(handler) {
      recoveryFailureListeners.add(handler);
      return () => recoveryFailureListeners.delete(handler);
    },
    close() {
      if (closed) return;
      closed = true;
      stopReplacement?.();
      for (const generation of generations) closeGeneration(generation);
      generations.clear();
      requestRegistrations.clear();
      eventListeners.clear();
      generationInvalidationListeners.clear();
      recoveryBaselineListeners.clear();
      recoveryFailureListeners.clear();
    },
  };

  function createGeneration(): HttpConnectionGeneration {
    const generationEndpointRevision = endpointRevision;
    const channel = createReliableHttpMessageChannel({
      endpointUrl: endpoint.endpointUrl,
      connectionId: options.connectionId,
      ...(endpoint.authToken ? { authToken: endpoint.authToken } : {}),
      ...(options.fetch ? { fetch: options.fetch } : {}),
      ...(options.retryDelayMs === undefined ? {} : { retryDelayMs: options.retryDelayMs }),
      ...(options.subscribeToWake ? { subscribeToWake: options.subscribeToWake } : {}),
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
    generation = { channel, connection, endpointRevision: generationEndpointRevision };
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

  async function initializeGeneration(
    generation: HttpConnectionGeneration,
    allowServerChange = false,
  ) {
    const params = initializeParams;
    if (!params) throw new Error("Backend connection is not initialized");
    const identity = await generation.channel.ready();
    if (initializedServerId && identity.serverId !== initializedServerId && !allowServerChange) {
      throw new Error("App Server instance changed while replacing an expired HTTP session");
    }
    initializedServerId = identity.serverId;
    return generation.connection.initialize(params, initializeMeta);
  }

  function replaceEndpoint(next: { endpointUrl: string; authToken: string }) {
    if (closed || (
      endpoint.endpointUrl === next.endpointUrl
      && endpoint.authToken === next.authToken
    )) return;
    endpoint = next;
    endpointRevision += 1;
    if (recoveryPromise) {
      // Abort an obsolete in-flight open so a newly published process endpoint
      // does not wait behind the operating system's connection timeout.
      if (recoveringGeneration && recoveringGeneration.endpointRevision !== endpointRevision) {
        closeGeneration(recoveringGeneration);
      }
      return;
    }
    if (!initializeParams) {
      const previous = active;
      active = createGeneration();
      bindGeneration(active);
      closeGeneration(previous);
      return;
    }
    beginRecovery(active, {
      reason: "appServerRestarted",
      message: "App Server process restarted",
    }, true);
  }

  function handleGenerationError(generation: HttpConnectionGeneration, error: unknown) {
    if (closed || generation !== active) return;
    const invalidation = isReliableHttpSessionExpired(error)
      ? {
          reason: "httpSessionExpired" as const,
          message: "HTTP RPC session expired",
        }
      : isReliableHttpReplayExpired(error)
        ? {
            reason: "serverReplayExpired" as const,
            message: "HTTP RPC server replay history expired",
          }
        : undefined;
    if (!invalidation || !initializeParams) {
      terminalError = error;
      return;
    }
    beginRecovery(generation, invalidation);
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
    beginRecovery(generation, {
      reason: "clientLivenessExpired",
      message: "App Server client liveness expired",
    });
  }

  function beginRecovery(
    generation: HttpConnectionGeneration,
    invalidation: BackendGenerationInvalidation & { message: string },
    allowServerChange = false,
  ) {
    if (recoveryPromise) return;
    terminalError = undefined;
    console.info(`[OpenAIDE] ${invalidation.message}; reinitializing the connection`);
    const attempt = recoverGeneration(generation, allowServerChange);
    recoveryPromise = attempt;
    void attempt.then(
      (result) => {
        if (recoveryPromise === attempt) recoveryPromise = undefined;
        console.info("[OpenAIDE] App Server connection reinitialized after expiry");
        notifyListeners(recoveryBaselineListeners, { reason: invalidation.reason, result });
      },
      (recoveryError) => {
        if (recoveryPromise === attempt) recoveryPromise = undefined;
        terminalError = recoveryError;
        console.error("[OpenAIDE] Failed to restore App Server connection after expiry", recoveryError);
        notifyListeners(recoveryFailureListeners, {
          reason: invalidation.reason,
          error: recoveryError,
        });
      },
    );
    notifyListeners(generationInvalidationListeners, { reason: invalidation.reason });
  }

  async function recoverGeneration(
    previous: HttpConnectionGeneration,
    allowServerChange: boolean,
  ) {
    let permitsChangedServer = allowServerChange;
    while (!closed) {
      const replacement = createGeneration();
      recoveringGeneration = replacement;
      try {
        const result = await initializeGeneration(replacement, permitsChangedServer);
        if (replacement.endpointRevision !== endpointRevision) {
          closeGeneration(replacement);
          permitsChangedServer = true;
          continue;
        }
        active = replacement;
        bindGeneration(replacement);
        closeGeneration(previous);
        return result;
      } catch (error) {
        closeGeneration(replacement);
        if (replacement.endpointRevision !== endpointRevision) {
          permitsChangedServer = true;
          continue;
        }
        throw error;
      } finally {
        if (recoveringGeneration === replacement) recoveringGeneration = undefined;
      }
    }
    throw new Error("Backend connection is closed");
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
  endpointRevision: number;
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
  const generationInvalidationListeners = new Set<
    (event: BackendGenerationInvalidation) => void
  >();
  const recoveryBaselineListeners = new Set<(event: BackendRecoveryBaseline) => void>();
  const recoveryFailureListeners = new Set<(event: BackendRecoveryFailure) => void>();
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
    handleGenerationInvalidated(handler) {
      generationInvalidationListeners.add(handler);
      return () => generationInvalidationListeners.delete(handler);
    },
    handleRecoveryBaseline(handler) {
      recoveryBaselineListeners.add(handler);
      return () => recoveryBaselineListeners.delete(handler);
    },
    handleRecoveryFailed(handler) {
      recoveryFailureListeners.add(handler);
      return () => recoveryFailureListeners.delete(handler);
    },
    close() {
      if (heartbeat) clearInterval(heartbeat);
      heartbeat = undefined;
      initialized = false;
      peer.close();
      options.channel.close?.();
      eventListeners.clear();
      generationInvalidationListeners.clear();
      recoveryBaselineListeners.clear();
      recoveryFailureListeners.clear();
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

function notifyListeners<T>(listeners: Iterable<(event: T) => void>, event: T) {
  for (const listener of listeners) {
    try {
      listener(event);
    } catch (error) {
      // Recovery ownership must not depend on the health of an independent observer.
      console.error("[OpenAIDE] Backend lifecycle listener failed", error);
    }
  }
}
