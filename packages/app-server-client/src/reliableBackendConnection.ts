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
import { createDiagnosticsLogger, type DiagnosticsLogger } from "./diagnostics.js";
import {
  CLIENT_INITIALIZE,
  type InitializeParams,
  type InitializeResult,
  type RequestMeta,
} from "./generated/protocol.js";
import { createHttpLinkOpener, type ReliableHttpFetch } from "./httpTransportLink.js";
import { AppServerProtocolError } from "./protocolError.js";
import {
  createReliableSessionChannel,
  type ReliableSessionChannel,
} from "./reliableSessionChannel.js";
import {
  createInternalReliableBackendConnection,
  diagnosticErrorFields,
  notifyListeners,
} from "./rpcBackendConnection.js";
import { transportLinkCloseKind, type TransportLinkOpener } from "./transportLink.js";
import {
  createWebSocketLinkOpener,
  type TransportWebSocket,
} from "./webSocketTransportLink.js";

export {
  createReliableBackendConnection,
  type ReliableBackendConnectionOptions,
} from "./rpcBackendConnection.js";

/** The carrier of the reliable session. Recovery behaves the same on both. */
export type ReliableBackendTransport = "http" | "webSocket";

export type ReliableLocalHttpBackendConnectionOptions = {
  endpointUrl: string;
  authToken: string;
  connectionId: string;
  /** Defaults to HTTP, which every supported host and network can carry. */
  transport?: ReliableBackendTransport;
  fetch?: ReliableHttpFetch;
  /** Supplies the socket for the WebSocket transport; defaults to the global one. */
  createWebSocket?: (url: string) => TransportWebSocket;
  heartbeatIntervalMs?: number;
  retryDelayMs?: number;
  receiveTimeoutMs?: number;
  maxConsecutiveReceiveTimeouts?: number;
  logger?: DiagnosticsLogger;
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
  return createAppServerSession(
    createReliableHttpBackendConnection(options),
    options.logger,
  );
}

export function createReliableWebProxyBackendConnection(
  options: ReliableWebProxyBackendConnectionOptions,
): AppServerSession {
  return createAppServerSession(
    createReliableHttpBackendConnection(options, true),
    options.logger,
  );
}

function createReliableHttpBackendConnection(
  options: ReliableLocalHttpBackendConnectionOptions | ReliableWebProxyBackendConnectionOptions,
  serverRestartBehindProxy = false,
): BackendConnection {
  const logger = options.logger ?? createDiagnosticsLogger();
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
  let lastInvalidation: (BackendGenerationInvalidation & { message: string }) | undefined;
  let closed = false;
  logger.info("backend_connection_created", {
    connection_id: options.connectionId,
  });
  bindGeneration(active);
  const stopReplacement = "subscribeToReplacement" in options
    ? options.subscribeToReplacement?.(replaceEndpoint)
    : undefined;

  return {
    retryRecovery() {
      if (closed) return false;
      if (recoveryPromise) return true;
      if (!terminalError || !lastInvalidation) return false;
      beginRecovery(active, lastInvalidation);
      return true;
    },
    initialize(params, meta) {
      if (initializePromise) return initializePromise;
      const startedAt = Date.now();
      logger.info("backend_initialize_started", {
        connection_id: options.connectionId,
        has_client_request_id: Boolean(meta?.clientRequestId),
      });
      initializeParams = params;
      initializeMeta = meta;
      const generation = active;
      initializePromise = initializeGeneration(generation).catch(async (error) => {
        // Expiry can race the first initialization response. In that case the
        // caller observes the replacement initialization, not a stale failure.
        if (recoveryPromise) return recoveryPromise;
        if (terminalError) throw terminalError;
        if (!closed && active !== generation) {
          // Closing the old peer can reject through several async wrappers after
          // recovery has completed. This is the replacement's cached result.
          return active.connection.initialize(params, meta);
        }
        logger.warn("backend_initialize_failed", {
          connection_id: options.connectionId,
          duration_ms: Date.now() - startedAt,
          ...diagnosticErrorFields(error),
        });
        throw error;
      });
      void initializePromise.then(
        () => logger.info("backend_initialize_completed", {
          connection_id: options.connectionId,
          duration_ms: Date.now() - startedAt,
        }),
        () => undefined,
      );
      return initializePromise;
    },
    async request(method, params, meta) {
      if (closed) throw new Error("Backend connection is closed");
      if (terminalError) throw terminalError;
      if (!initializeParams) throw new Error("Backend connection is not initialized");
      // Requests created after expiry wait for the fresh initialized session.
      // Ambiguous requests already sent through a lost transport are never replayed.
      if (recoveryPromise) {
        const startedAt = Date.now();
        logger.info("backend_request_waiting_for_recovery", {
          connection_id: options.connectionId,
          method,
        });
        await recoveryPromise;
        logger.info("backend_request_recovery_wait_completed", {
          connection_id: options.connectionId,
          method,
          duration_ms: Date.now() - startedAt,
        });
      }
      if (terminalError) throw terminalError;
      const generation = active;
      try {
        return await generation.connection.request(method, params, meta);
      } catch (error) {
        const recovery = recoveryPromise;
        if (!isNotInitialized(error)) throw error;
        logger.warn("backend_request_rejected_before_dispatch", {
          connection_id: options.connectionId,
          method,
          reason: "client_liveness_expired",
        });
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
      logger.info("backend_connection_closed", {
        connection_id: options.connectionId,
        generation_count: generations.size,
      });
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
    logger.info("backend_transport_generation_created", {
      connection_id: options.connectionId,
      endpoint_revision: generationEndpointRevision,
      transport: options.transport ?? "http",
    });
    const channel = createReliableSessionChannel({
      openLink: createLinkOpener(),
      connectionId: options.connectionId,
      logger,
      ...(options.retryDelayMs === undefined ? {} : { retryDelayMs: options.retryDelayMs }),
    });
    let generation: HttpConnectionGeneration;
    const connection = createInternalReliableBackendConnection({
      channel,
      connectionId: options.connectionId,
      logger,
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

  /** The only place that knows which carrier a generation's session uses. */
  function createLinkOpener(): TransportLinkOpener {
    if (options.transport === "webSocket") {
      return createWebSocketLinkOpener({
        endpointUrl: endpoint.endpointUrl,
        connectionId: options.connectionId,
        logger,
        ...(endpoint.authToken ? { authToken: endpoint.authToken } : {}),
        ...(options.createWebSocket ? { createSocket: options.createWebSocket } : {}),
        ...(options.subscribeToWake ? { subscribeToWake: options.subscribeToWake } : {}),
      });
    }
    return createHttpLinkOpener({
      endpointUrl: endpoint.endpointUrl,
      connectionId: options.connectionId,
      deferReceiveUntilFirstUpload: true,
      ...(endpoint.authToken ? { authToken: endpoint.authToken } : {}),
      ...(options.fetch ? { fetch: options.fetch } : {}),
      ...(options.retryDelayMs === undefined ? {} : { retryDelayMs: options.retryDelayMs }),
      ...(options.receiveTimeoutMs === undefined
        ? {}
        : { receiveTimeoutMs: options.receiveTimeoutMs }),
      ...(options.maxConsecutiveReceiveTimeouts === undefined
        ? {}
        : { maxConsecutiveReceiveTimeouts: options.maxConsecutiveReceiveTimeouts }),
      logger,
      ...(options.subscribeToWake ? { subscribeToWake: options.subscribeToWake } : {}),
    });
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
      logger.warn("backend_server_identity_changed_during_recovery", {
        connection_id: options.connectionId,
        previous_server_id: initializedServerId,
        next_server_id: identity.serverId,
        allow_server_change: allowServerChange,
      });
      throw new Error("App Server instance changed while replacing an expired HTTP session");
    }
    const result = await generation.connection.initialize(params, initializeMeta);
    initializedServerId = identity.serverId;
    return result;
  }

  function replaceEndpoint(next: { endpointUrl: string; authToken: string }) {
    if (closed || (
      endpoint.endpointUrl === next.endpointUrl
      && endpoint.authToken === next.authToken
    )) return;
    endpoint = next;
    endpointRevision += 1;
    logger.info("backend_endpoint_replaced", {
      connection_id: options.connectionId,
      endpoint_revision: endpointRevision,
      recovery_in_progress: Boolean(recoveryPromise),
    });
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
    let invalidation: (BackendGenerationInvalidation & { message: string }) | undefined;
    const closeKind = transportLinkCloseKind(error);
    if (closeKind === "sessionExpired") {
      // TODO: rename `httpSessionExpired` in BackendGenerationInvalidation to a
      // carrier-neutral reason; it now also covers a WebSocket session and the
      // name leaks one transport into every consumer of the invalidation.
      invalidation = {
        reason: "httpSessionExpired",
        message: "RPC session expired",
      };
    } else if (closeKind === "replayExpired") {
      invalidation = {
        reason: "serverReplayExpired",
        message: "RPC server replay history expired",
      };
    } else if (closeKind === "receiveStalled") {
      invalidation = {
        reason: "receiveStalled",
        message: "RPC receive remained stalled",
      };
    }
    if (!invalidation || !initializeParams) {
      terminalError = error;
      logger.error("backend_transport_failed_terminal", {
        connection_id: options.connectionId,
        ...diagnosticErrorFields(error),
      });
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
    logger.warn("backend_generation_invalidated", {
      connection_id: options.connectionId,
      reason: "clientLivenessExpired",
      ...diagnosticErrorFields(error),
    });
    beginRecovery(generation, {
      reason: "clientLivenessExpired",
      message: "App Server client liveness expired",
    });
  }

  function beginRecovery(
    generation: HttpConnectionGeneration,
    invalidation: BackendGenerationInvalidation & { message: string },
    allowServerChange = serverRestartBehindProxy || invalidation.reason === "appServerRestarted",
  ) {
    if (recoveryPromise) return;
    lastInvalidation = invalidation;
    terminalError = undefined;
    const previousServerId = initializedServerId;
    const startedAt = Date.now();
    logger.warn("backend_recovery_started", {
      connection_id: options.connectionId,
      reason: invalidation.reason,
      allow_server_change: allowServerChange,
    });
    const attempt = recoverGeneration(generation, allowServerChange);
    recoveryPromise = attempt;
    void attempt.then(
      (result) => {
        if (recoveryPromise === attempt) recoveryPromise = undefined;
        const reason = previousServerId && result.snapshot.server.serverId !== previousServerId
          ? "appServerRestarted"
          : invalidation.reason;
        logger.info("backend_recovery_completed", {
          connection_id: options.connectionId,
          reason,
          duration_ms: Date.now() - startedAt,
        });
        notifyListeners(
          recoveryBaselineListeners,
          { reason, result },
          logger,
          "recovery_baseline",
        );
      },
      (recoveryError) => {
        if (recoveryPromise === attempt) recoveryPromise = undefined;
        terminalError = recoveryError;
        logger.error("backend_recovery_failed", {
          connection_id: options.connectionId,
          reason: invalidation.reason,
          duration_ms: Date.now() - startedAt,
          ...diagnosticErrorFields(recoveryError),
        });
        notifyListeners(
          recoveryFailureListeners,
          { reason: invalidation.reason, error: recoveryError },
          logger,
          "recovery_failure",
        );
      },
    );
    notifyListeners(
      generationInvalidationListeners,
      { reason: invalidation.reason },
      logger,
      "generation_invalidation",
    );
  }

  async function recoverGeneration(
    previous: HttpConnectionGeneration,
    allowServerChange: boolean,
  ) {
    let permitsChangedServer = allowServerChange;
    let attemptNumber = 0;
    while (!closed) {
      attemptNumber += 1;
      const replacement = createGeneration();
      recoveringGeneration = replacement;
      const startedAt = Date.now();
      logger.info("backend_recovery_attempt_started", {
        connection_id: options.connectionId,
        attempt: attemptNumber,
        endpoint_revision: replacement.endpointRevision,
      });
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
        logger.info("backend_recovery_attempt_completed", {
          connection_id: options.connectionId,
          attempt: attemptNumber,
          duration_ms: Date.now() - startedAt,
        });
        return result;
      } catch (error) {
        logger.warn("backend_recovery_attempt_failed", {
          connection_id: options.connectionId,
          attempt: attemptNumber,
          duration_ms: Date.now() - startedAt,
          endpoint_changed: replacement.endpointRevision !== endpointRevision,
          ...diagnosticErrorFields(error),
        });
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
  channel: ReliableSessionChannel;
  connection: BackendConnection;
  endpointRevision: number;
  unsubscribeError?: BackendUnsubscribe;
  unsubscribeEvent?: BackendUnsubscribe;
};

function isNotInitialized(error: unknown) {
  return error instanceof AppServerProtocolError
    && error.protocolError.code === "notInitialized";
}
