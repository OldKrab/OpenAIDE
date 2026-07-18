import type {
  AppServerEvent,
  InitializeParams,
  InitializeResult,
  ProtocolMethod,
  RequestId,
  RequestMeta,
  RequestParamsByMethod,
  ResponseResultByMethod,
  ServerId,
  ServerRequestMethod,
  ServerRequestResponseResultByMethod,
  StateRootId,
  SubscriptionScope,
  SubscriptionSnapshot,
} from "./generated/protocol.js";

export type BackendEventListener = (event: AppServerEvent) => void;
export type BackendUnsubscribe = () => void;
export type BackendGenerationInvalidation = {
  reason: "httpSessionExpired" | "clientLivenessExpired" | "serverReplayExpired";
};
export type BackendRecoveryBaseline = BackendGenerationInvalidation & {
  result: InitializeResult;
};
export type BackendRecoveryFailure = BackendGenerationInvalidation & {
  error: unknown;
};
export type AppServerSessionStatus =
  | { status: "connecting"; generation: number }
  | { status: "ready"; generation: number }
  | { status: "recovering"; generation: number; reason: BackendGenerationInvalidation["reason"] }
  | { status: "unavailable"; generation: number; error: unknown };
export type AppServerStateObserver = {
  onSnapshot(snapshot: SubscriptionSnapshot, event?: AppServerEvent, snapshotChanged?: boolean): void;
  onBaselineLost?(): void;
  onBaselineReady?(): void;
  onBaselineError?(error: unknown): void;
};
export type BackendReplicaIdentity = {
  serverId: ServerId;
  stateRootId: StateRootId;
};
export type BackendRequestContext = {
  requestId: RequestId;
  scope?: unknown;
  signal: AbortSignal;
};

export interface BackendConnection {
  initialize(params: InitializeParams, meta?: RequestMeta): Promise<InitializeResult>;
  request<M extends ProtocolMethod>(
    method: M,
    params: RequestParamsByMethod[M],
    meta?: RequestMeta,
  ): Promise<ResponseResultByMethod[M]>;
  /** Registers a typed inbound request handler; its return value is the protocol response. */
  handleRequest<M extends ServerRequestMethod>(
    method: M,
    handler: (
      params: import("./generated/protocol.js").ServerRequestParamsByMethod[M],
      context: BackendRequestContext,
    ) => Promise<ServerRequestResponseResultByMethod[M]> | ServerRequestResponseResultByMethod[M],
  ): BackendUnsubscribe;
  /** Registers a typed App Server event notification handler. */
  handleNotification(method: "app/event", handler: BackendEventListener): BackendUnsubscribe;
  /** Signals that active state subscriptions must install fresh authoritative baselines. */
  handleGenerationInvalidated(
    handler: (event: BackendGenerationInvalidation) => void,
  ): BackendUnsubscribe;
  /** Publishes the authoritative initialization baseline installed by recovery. */
  handleRecoveryBaseline(
    handler: (event: BackendRecoveryBaseline) => void,
  ): BackendUnsubscribe;
  /** Reports a terminal replacement failure so logical-session barriers can settle. */
  handleRecoveryFailed(
    handler: (event: BackendRecoveryFailure) => void,
  ): BackendUnsubscribe;
  close(): Promise<void> | void;
}

/** One logical initialized client session across replaceable physical transports. */
export interface AppServerSession extends BackendConnection {
  /** Owns one scope replica, including cursor-gap and transport-generation recovery. */
  subscribeState(scope: SubscriptionScope, observer: AppServerStateObserver): BackendUnsubscribe;
  /** Reports the single request-readiness state for the logical session. */
  handleSessionStatus(handler: (status: AppServerSessionStatus) => void): BackendUnsubscribe;
}

export type BackendRequest<M extends ProtocolMethod> = {
  method: M;
  params: RequestParamsByMethod[M];
  meta?: RequestMeta;
};

export function backendRequest<M extends ProtocolMethod>(
  method: M,
  params: RequestParamsByMethod[M],
  meta?: RequestMeta,
): BackendRequest<M> {
  return meta === undefined ? { method, params } : { method, params, meta };
}
