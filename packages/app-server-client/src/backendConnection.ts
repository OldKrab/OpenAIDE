import type { NavigationReplica } from "./stateIngestionTaskNavigation.js";
import type {
  AppServerEvent,
  ClientProbeResult,
  InitializeParams,
  InitializeResult,
  NavigationViewParams,
  ProtocolMethod,
  RequestId,
  RequestMeta,
  RequestParamsByMethod,
  ResponseResultByMethod,
  ServerId,
  ServerRequestMethod,
  ServerRequestResponseResultByMethod,
  StateRootId,
  SubagentHistorySnapshot,
  SubscriptionScope,
  SubscriptionSnapshot,
  TaskId,
  TaskSnapshot,
  ToolDetailSnapshot,
} from "./generated/protocol.js";

export type BackendEventListener = (event: AppServerEvent) => void;
export type BackendUnsubscribe = () => void;
export type BackendGenerationInvalidation = {
  reason:
    | "sessionExpired"
    | "clientLivenessExpired"
    | "serverReplayExpired"
    | "receiveStalled"
    | "appServerRestarted";
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
/**
 * A Task replica's installed state: its complete current fields and Chat, and the Chat version
 * they were built at. A subscribe reply's `chatSync` is resolved into a complete Chat before any
 * observer sees it (ADR-0086), so it is not kept.
 */
export type TaskReplicaSnapshot = { kind: "task"; task: TaskSnapshot; chatVersion: number };
/**
 * A Tool detail replica's installed state: complete fields and the whole terminal output.
 * `details.revision` is the artifact sequence it was built at.
 */
export type ToolDetailReplicaSnapshot = {
  kind: "toolDetail";
  taskId: TaskId;
  artifactId: string;
  details: ToolDetailSnapshot;
};
/**
 * A Subagent History replica's installed state: complete fields and the Chat page it holds.
 * `history.revision` is the history revision it was built at.
 */
export type SubagentHistoryReplicaSnapshot = { kind: "subagentHistory"; history: SubagentHistorySnapshot };
/** An entity replica's installed state: what a subscribe reply resumes from (ADR-0086). */
export type EntityReplicaSnapshot =
  | TaskReplicaSnapshot
  | ToolDetailReplicaSnapshot
  | SubagentHistoryReplicaSnapshot;
/** A Navigation view's installed state: groups and rows exactly as App Server computed them. */
export type NavigationReplicaSnapshot = { kind: "taskNavigation"; navigation: NavigationReplica };
/** A scope replica's installed state. */
export type ReplicaSnapshot =
  | Exclude<SubscriptionSnapshot, { kind: "task" | "toolDetail" | "subagentHistory" }>
  | EntityReplicaSnapshot
  | NavigationReplicaSnapshot;
export type AppServerStateObserver = {
  onSnapshot(snapshot: ReplicaSnapshot, event?: AppServerEvent, snapshotChanged?: boolean): void;
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
  /**
   * Asks the App Server what it is without initializing: every App Server answers
   * `client/probe` before `client/initialize`, so a refused client can still learn the
   * App Server's versions and name them to the user.
   */
  probe?(): Promise<ClientProbeResult>;
  retryRecovery?(): boolean;
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
  /**
   * Reports that the App Server stopped answering, or answers again, while the
   * session itself stays open and keeps retrying. Presentation only: requests
   * are still accepted and delivered once the link is back.
   */
  handleResponsiveness?(handler: (responsive: boolean) => void): BackendUnsubscribe;
  /** Reports a terminal replacement failure so logical-session barriers can settle. */
  handleRecoveryFailed(
    handler: (event: BackendRecoveryFailure) => void,
  ): BackendUnsubscribe;
  close(): Promise<void> | void;
}

/** One logical initialized client session across replaceable physical transports. */
export interface AppServerSession extends BackendConnection {
  /**
   * Owns one scope replica, including cursor-gap and transport-generation recovery. A
   * `taskNavigation` scope needs `view`, the parameters of its Navigation view; later changes go
   * through `request(NAVIGATION_UPDATE, ...)`, which the session also remembers for recovery.
   */
  subscribeState(
    scope: SubscriptionScope,
    observer: AppServerStateObserver,
    view?: NavigationViewParams,
  ): BackendUnsubscribe;
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
