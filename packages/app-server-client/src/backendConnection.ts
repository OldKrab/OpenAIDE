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
} from "./generated/protocol.js";

export type BackendEventListener = (event: AppServerEvent) => void;
export type BackendUnsubscribe = () => void;
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
  close(): Promise<void> | void;
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
