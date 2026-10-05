import type {
  BackendConnection,
  BackendEventListener,
  BackendGenerationInvalidation,
  BackendRecoveryBaseline,
  BackendRecoveryFailure,
  BackendUnsubscribe,
} from "./backendConnection.js";
import { createDiagnosticsLogger, type DiagnosticsLogger } from "./diagnostics.js";
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
import { transportLinkErrorDiagnosticFields } from "./transportLink.js";

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
  connectionId?: string;
  logger?: DiagnosticsLogger;
};

export type InternalReliableBackendConnectionOptions = ReliableBackendConnectionOptions & {
  onRequestError?: (error: unknown, method: ProtocolMethod) => void;
};

/** Adapts the generated App Server contract onto the transport-independent peer. */
export function createReliableBackendConnection(
  options: ReliableBackendConnectionOptions,
): BackendConnection {
  return createInternalReliableBackendConnection(options);
}

/** Adds the request-failure hook the recovering connection classifies with. */
export function createInternalReliableBackendConnection(
  options: InternalReliableBackendConnectionOptions,
): BackendConnection {
  const logger = options.logger ?? createDiagnosticsLogger();
  const connectionContext = connectionDiagnosticFields(options.connectionId);
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
  let heartbeatFailureCount = 0;
  let heartbeatFailureActive = false;
  let heartbeatPending = false;
  let requestSequence = 0;

  // RpcPeer owns the single protocol handler. Backend consumers are independent
  // projections of that notification stream and therefore need local multicast.
  peer.handleNotification("app/event", (event) => {
    notifyListeners(eventListeners, event, logger, "app_event");
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
      logger.info("backend_rpc_connection_closed", connectionContext);
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
    const operationId = `client-rpc-${++requestSequence}`;
    const logRequest = method !== CLIENT_HEARTBEAT;
    const startedAt = Date.now();
    if (logRequest) {
      logger.info("backend_rpc_request_started", {
        ...connectionContext,
        operation_id: operationId,
        method,
        has_client_request_id: Boolean(meta?.clientRequestId),
      });
    }
    try {
      const response = await peer.request(method, params, meta === undefined ? undefined : {
        meta,
      }) as unknown as ResponseEnvelope<
        ResponseResultByMethod[M]
      >;
      if (logRequest) {
        logger.info("backend_rpc_request_completed", {
          ...connectionContext,
          operation_id: operationId,
          method,
          duration_ms: Date.now() - startedAt,
        });
      }
      return response.result;
    } catch (error) {
      let requestError = error;
      if (error instanceof RpcResponseError) {
        const envelope = errorEnvelopeFromUnknown(error.responseError);
        if (envelope) requestError = new AppServerProtocolError(envelope);
      }
      options.onRequestError?.(requestError, method);
      if (logRequest) {
        logger.warn("backend_rpc_request_failed", {
          ...connectionContext,
          operation_id: operationId,
          method,
          duration_ms: Date.now() - startedAt,
          ...diagnosticErrorFields(requestError),
        });
      }
      throw requestError;
    }
  }

  function startHeartbeat() {
    if (heartbeat) clearInterval(heartbeat);
    heartbeat = setInterval(() => {
      if (!initialized || heartbeatPending) return;
      heartbeatPending = true;
      void sendRequest(CLIENT_HEARTBEAT, {})
        .then(() => {
          if (!heartbeatFailureActive) return;
          const recoveredAfterFailureCount = heartbeatFailureCount;
          heartbeatFailureActive = false;
          heartbeatFailureCount = 0;
          logger.info("backend_heartbeat_recovered", {
            ...connectionContext,
            failure_count: recoveredAfterFailureCount,
          });
        })
        .catch((error) => {
          heartbeatFailureCount += 1;
          if (!heartbeatFailureActive) {
            heartbeatFailureActive = true;
            logger.warn("backend_heartbeat_failed", {
              ...connectionContext,
              failure_count: heartbeatFailureCount,
              ...diagnosticErrorFields(error),
            });
          }
        })
        .finally(() => {
          heartbeatPending = false;
        });
    }, options.heartbeatIntervalMs ?? 5_000);
  }
}

export function diagnosticErrorFields(error: unknown) {
  return {
    error_kind: error instanceof Error && error.name ? error.name : typeof error,
    ...(error instanceof AppServerProtocolError
      ? { error_code: error.protocolError.code }
      : {}),
    ...transportLinkErrorDiagnosticFields(error),
  };
}

function connectionDiagnosticFields(connectionId: string | undefined) {
  return connectionId === undefined ? {} : { connection_id: connectionId };
}

export function notifyListeners<T>(
  listeners: Iterable<(event: T) => void>,
  event: T,
  logger: DiagnosticsLogger,
  listenerKind: string,
) {
  for (const listener of listeners) {
    try {
      listener(event);
    } catch (error) {
      // Recovery ownership must not depend on the health of an independent observer.
      logger.error("backend_lifecycle_listener_failed", {
        listener_kind: listenerKind,
        error_kind: error instanceof Error && error.name ? error.name : typeof error,
      });
    }
  }
}