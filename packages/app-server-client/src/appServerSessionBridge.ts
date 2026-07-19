import {
  AppServerProtocolError,
  errorEnvelopeFromUnknown,
} from "./protocolError.js";
import type {
  AppServerSession,
  AppServerSessionStatus,
  AppServerStateObserver,
  BackendEventListener,
  BackendGenerationInvalidation,
  BackendRecoveryBaseline,
  BackendRecoveryFailure,
  BackendRequestContext,
  BackendUnsubscribe,
} from "./backendConnection.js";
import type {
  AppServerEvent,
  ErrorEnvelope,
  InitializeParams,
  RequestMeta,
  ServerRequestMethod,
  SubscriptionScope,
  SubscriptionSnapshot,
} from "./generated/protocol.js";

export type AppServerSessionViewMessage =
  | { type: "appServer.session.initialize"; requestId: string; params?: InitializeParams; meta?: RequestMeta }
  | { type: "appServer.session.request"; requestId: string; method: string; params: unknown; meta?: RequestMeta }
  | { type: "appServer.session.subscribe"; subscriptionId: string; scope: SubscriptionScope }
  | { type: "appServer.session.unsubscribe"; subscriptionId: string }
  | { type: "appServer.session.registerRequestHandler"; method: ServerRequestMethod }
  | { type: "appServer.session.unregisterRequestHandler"; method: ServerRequestMethod }
  | { type: "appServer.session.serverResponse"; requestId: string; result?: unknown; error?: SerializedBridgeError }
  | { type: "appServer.session.detach" };

export type AppServerSessionHostMessage =
  | { type: "appServer.session.response"; requestId: string; result?: unknown; error?: SerializedBridgeError }
  | { type: "appServer.session.snapshot"; subscriptionId: string; snapshot: SubscriptionSnapshot; event?: AppServerEvent; snapshotChanged?: boolean }
  | { type: "appServer.session.baselineLost"; subscriptionId: string }
  | { type: "appServer.session.baselineReady"; subscriptionId: string }
  | { type: "appServer.session.baselineError"; subscriptionId: string; error: SerializedBridgeError }
  | { type: "appServer.session.notification"; event: AppServerEvent }
  | { type: "appServer.session.generationInvalidated"; event: BackendGenerationInvalidation }
  | { type: "appServer.session.recoveryBaseline"; event: BackendRecoveryBaseline }
  | { type: "appServer.session.recoveryFailed"; event: Omit<BackendRecoveryFailure, "error"> & { error: SerializedBridgeError } }
  | { type: "appServer.session.status"; status: SerializedSessionStatus }
  | { type: "appServer.session.serverRequest"; requestId: string; method: ServerRequestMethod; params: unknown; context: { requestId: string; scope?: unknown } };

export type AppServerSessionBridgePort = {
  post(message: AppServerSessionViewMessage): void;
  subscribe(listener: (message: AppServerSessionHostMessage) => void): BackendUnsubscribe;
};

type SerializedBridgeError = {
  name?: string;
  message: string;
  envelope?: ErrorEnvelope;
};

type SerializedSessionStatus = Exclude<AppServerSessionStatus, { status: "unavailable" }>
  | { status: "unavailable"; generation: number; error: SerializedBridgeError };

type PendingRequest = {
  resolve(value: unknown): void;
  reject(error: unknown): void;
};

/** Implements the normal AppServerSession port over an App Shell-owned message bridge. */
export function createBridgedAppServerSession(port: AppServerSessionBridgePort): AppServerSession {
  const pending = new Map<string, PendingRequest>();
  const observers = new Map<string, AppServerStateObserver>();
  const requestHandlers = new Map<ServerRequestMethod, (
    params: never,
    context: BackendRequestContext,
  ) => Promise<unknown> | unknown>();
  const notificationListeners = new Set<BackendEventListener>();
  const invalidationListeners = new Set<(event: BackendGenerationInvalidation) => void>();
  const recoveryBaselineListeners = new Set<(event: BackendRecoveryBaseline) => void>();
  const recoveryFailureListeners = new Set<(event: BackendRecoveryFailure) => void>();
  const statusListeners = new Set<(status: AppServerSessionStatus) => void>();
  let nextRequestId = 1;
  let nextSubscriptionId = 1;
  let closed = false;
  let latestStatus: AppServerSessionStatus = { status: "connecting", generation: 0 };
  const stopMessages = port.subscribe(handleMessage);

  return {
    initialize(params, meta) {
      return sendRequest("appServer.session.initialize", { params, meta }) as never;
    },
    request(method, params, meta) {
      return sendRequest("appServer.session.request", { method, params, meta }) as never;
    },
    handleRequest(method, handler) {
      requestHandlers.set(method, handler as never);
      port.post({ type: "appServer.session.registerRequestHandler", method });
      return () => {
        if (requestHandlers.get(method) !== handler) return;
        requestHandlers.delete(method);
        port.post({ type: "appServer.session.unregisterRequestHandler", method });
      };
    },
    handleNotification(_method, handler) {
      notificationListeners.add(handler);
      return () => notificationListeners.delete(handler);
    },
    handleGenerationInvalidated(handler) {
      invalidationListeners.add(handler);
      return () => invalidationListeners.delete(handler);
    },
    handleRecoveryBaseline(handler) {
      recoveryBaselineListeners.add(handler);
      return () => recoveryBaselineListeners.delete(handler);
    },
    handleRecoveryFailed(handler) {
      recoveryFailureListeners.add(handler);
      return () => recoveryFailureListeners.delete(handler);
    },
    subscribeState(scope, observer) {
      const subscriptionId = `subscription-${nextSubscriptionId++}`;
      observers.set(subscriptionId, observer);
      port.post({ type: "appServer.session.subscribe", subscriptionId, scope });
      return () => {
        if (observers.get(subscriptionId) !== observer) return;
        observers.delete(subscriptionId);
        port.post({ type: "appServer.session.unsubscribe", subscriptionId });
      };
    },
    handleSessionStatus(handler) {
      statusListeners.add(handler);
      handler(latestStatus);
      return () => statusListeners.delete(handler);
    },
    close() {
      if (closed) return;
      closed = true;
      port.post({ type: "appServer.session.detach" });
      stopMessages();
      rejectPending(new Error("App Server view detached"));
      observers.clear();
      requestHandlers.clear();
      notificationListeners.clear();
      invalidationListeners.clear();
      recoveryBaselineListeners.clear();
      recoveryFailureListeners.clear();
      statusListeners.clear();
    },
  };

  function sendRequest(
    type: "appServer.session.initialize" | "appServer.session.request",
    payload: Record<string, unknown>,
  ) {
    if (closed) return Promise.reject(new Error("App Server view is detached"));
    const requestId = `request-${nextRequestId++}`;
    const result = new Promise((resolve, reject) => pending.set(requestId, { resolve, reject }));
    port.post({ type, requestId, ...payload } as AppServerSessionViewMessage);
    return result;
  }

  function handleMessage(message: AppServerSessionHostMessage) {
    if (closed) return;
    if (message.type === "appServer.session.response") {
      const request = pending.get(message.requestId);
      if (!request) return;
      pending.delete(message.requestId);
      if (message.error) request.reject(deserializeBridgeError(message.error));
      else request.resolve(message.result);
      return;
    }
    if (message.type === "appServer.session.snapshot") {
      observers.get(message.subscriptionId)?.onSnapshot(
        message.snapshot,
        message.event,
        message.snapshotChanged,
      );
      return;
    }
    if (message.type === "appServer.session.baselineLost") {
      observers.get(message.subscriptionId)?.onBaselineLost?.();
      return;
    }
    if (message.type === "appServer.session.baselineReady") {
      observers.get(message.subscriptionId)?.onBaselineReady?.();
      return;
    }
    if (message.type === "appServer.session.baselineError") {
      observers.get(message.subscriptionId)?.onBaselineError?.(deserializeBridgeError(message.error));
      return;
    }
    if (message.type === "appServer.session.notification") {
      for (const listener of notificationListeners) listener(message.event);
      return;
    }
    if (message.type === "appServer.session.generationInvalidated") {
      for (const listener of invalidationListeners) listener(message.event);
      return;
    }
    if (message.type === "appServer.session.recoveryBaseline") {
      for (const listener of recoveryBaselineListeners) listener(message.event);
      return;
    }
    if (message.type === "appServer.session.recoveryFailed") {
      const event = { ...message.event, error: deserializeBridgeError(message.event.error) };
      for (const listener of recoveryFailureListeners) listener(event);
      return;
    }
    if (message.type === "appServer.session.status") {
      latestStatus = deserializeSessionStatus(message.status);
      for (const listener of statusListeners) listener(latestStatus);
      return;
    }
    if (message.type === "appServer.session.serverRequest") {
      void respondToServerRequest(message);
    }
  }

  async function respondToServerRequest(
    message: Extract<AppServerSessionHostMessage, { type: "appServer.session.serverRequest" }>,
  ) {
    const handler = requestHandlers.get(message.method);
    if (!handler) {
      port.post({
        type: "appServer.session.serverResponse",
        requestId: message.requestId,
        error: serializeBridgeError(new Error(`No handler registered for ${message.method}`)),
      });
      return;
    }
    try {
      const result = await handler(message.params as never, {
        requestId: message.context.requestId as BackendRequestContext["requestId"],
        scope: message.context.scope,
        signal: new AbortController().signal,
      });
      port.post({ type: "appServer.session.serverResponse", requestId: message.requestId, result });
    } catch (error) {
      port.post({
        type: "appServer.session.serverResponse",
        requestId: message.requestId,
        error: serializeBridgeError(error),
      });
    }
  }

  function rejectPending(error: Error) {
    for (const request of pending.values()) request.reject(error);
    pending.clear();
  }
}

export function isAppServerSessionViewMessage(value: unknown): value is AppServerSessionViewMessage {
  return Boolean(value)
    && typeof value === "object"
    && typeof (value as { type?: unknown }).type === "string"
    && (value as { type: string }).type.startsWith("appServer.session.");
}

export function isAppServerSessionHostMessage(value: unknown): value is AppServerSessionHostMessage {
  return Boolean(value)
    && typeof value === "object"
    && typeof (value as { type?: unknown }).type === "string"
    && (value as { type: string }).type.startsWith("appServer.session.");
}

export function serializeBridgeError(error: unknown): SerializedBridgeError {
  const envelope = error instanceof AppServerProtocolError
    ? error.envelope
    : errorEnvelopeFromUnknown(error);
  return {
    name: error instanceof Error ? error.name : undefined,
    message: error instanceof Error ? error.message : String(error),
    ...(envelope ? { envelope } : {}),
  };
}

function deserializeBridgeError(error: SerializedBridgeError): Error {
  if (error.envelope) return new AppServerProtocolError(error.envelope);
  const result = new Error(error.message);
  if (error.name) result.name = error.name;
  return result;
}

export function serializeSessionStatus(status: AppServerSessionStatus): SerializedSessionStatus {
  return status.status === "unavailable"
    ? { ...status, error: serializeBridgeError(status.error) }
    : status;
}

function deserializeSessionStatus(status: SerializedSessionStatus): AppServerSessionStatus {
  return status.status === "unavailable"
    ? { ...status, error: deserializeBridgeError(status.error) }
    : status;
}
