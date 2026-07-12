import type {
  AppServerEvent,
  InitializeParams,
  InitializeResult,
  ProtocolMethod,
  RequestId,
  RequestMeta,
  RequestParamsByMethod,
  ResponseResultByMethod,
  ServerRequestMethod,
  ServerRequestResponseResultByMethod,
  TypedServerRequest,
} from "./generated/protocol.js";

export type BackendEventListener = (event: AppServerEvent) => void;
export type BackendServerRequestListener = (
  request: TypedServerRequest<ServerRequestMethod>,
) => void;
export type BackendUnsubscribe = () => void;
export type BackendStateResetListener = () => void;

export interface BackendConnection {
  initialize(params: InitializeParams, meta?: RequestMeta): Promise<InitializeResult>;
  request<M extends ProtocolMethod>(
    method: M,
    params: RequestParamsByMethod[M],
    meta?: RequestMeta,
  ): Promise<ResponseResultByMethod[M]>;
  events(listener: BackendEventListener): BackendUnsubscribe;
  /** Fires when event-stream continuity was lost and watched state needs a fresh snapshot. */
  stateResets(listener: BackendStateResetListener): BackendUnsubscribe;
  serverRequests(listener: BackendServerRequestListener): BackendUnsubscribe;
  respond<M extends ServerRequestMethod>(
    requestId: RequestId,
    result: ServerRequestResponseResultByMethod[M],
  ): Promise<void> | void;
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
