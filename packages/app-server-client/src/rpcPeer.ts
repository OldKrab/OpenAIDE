export type RpcRequestSpec<Params = unknown, Result = unknown> = {
  params: Params;
  result: Result;
};

export type RpcRequestMap = Record<string, RpcRequestSpec>;
export type RpcNotificationMap = Record<string, { params: unknown }>;
export type RpcId = string | number;

export type RpcMessage =
  | {
      jsonrpc: "2.0";
      id: RpcId;
      method: string;
      params?: unknown;
      meta?: unknown;
      scope?: unknown;
    }
  | { jsonrpc: "2.0"; method: string; params?: unknown }
  | { jsonrpc: "2.0"; id: RpcId; result?: unknown; error?: unknown };

export type RpcMessageError = {
  code: number;
  message: string;
  data?: unknown;
};

export type RpcMessageChannel = {
  send(message: RpcMessage): void;
  subscribe(receive: (message: RpcMessage) => void): () => void;
  /** Reports authoritative channel loss; transient reconnects stay hidden. */
  subscribeErrors?(receive: (error: unknown) => void): () => void;
};

export type RpcRequestContext = {
  requestId: RpcId;
  scope?: unknown;
  signal: AbortSignal;
};

type RequestHandler = (params: unknown, context: RpcRequestContext) => unknown | Promise<unknown>;
type NotificationHandler = (params: unknown) => void | Promise<void>;
type PendingRequest = {
  resolve(value: unknown): void;
  reject(error: unknown): void;
};

/**
 * Presents symmetric request and notification semantics while hiding response
 * correlation and handler-owned response construction from product callers.
 */
export interface RpcPeer<
  OutgoingRequests extends RpcRequestMap,
  OutgoingNotifications extends RpcNotificationMap,
  IncomingRequests extends RpcRequestMap,
  IncomingNotifications extends RpcNotificationMap,
> {
  request<M extends keyof OutgoingRequests & string>(
    method: M,
    params: OutgoingRequests[M]["params"],
    options?: { signal?: AbortSignal; meta?: unknown },
  ): Promise<OutgoingRequests[M]["result"]>;
  notify<M extends keyof OutgoingNotifications & string>(
    method: M,
    params: OutgoingNotifications[M]["params"],
  ): void;
  handleRequest<M extends keyof IncomingRequests & string>(
    method: M,
    handler: (
      params: IncomingRequests[M]["params"],
      context: RpcRequestContext,
    ) => IncomingRequests[M]["result"] | Promise<IncomingRequests[M]["result"]>,
  ): () => void;
  handleNotification<M extends keyof IncomingNotifications & string>(
    method: M,
    handler: (params: IncomingNotifications[M]["params"]) => void | Promise<void>,
  ): () => void;
  close(): void;
}

export function createRpcPeer<
  OutgoingRequests extends RpcRequestMap,
  OutgoingNotifications extends RpcNotificationMap,
  IncomingRequests extends RpcRequestMap,
  IncomingNotifications extends RpcNotificationMap,
>(channel: RpcMessageChannel): RpcPeer<
  OutgoingRequests,
  OutgoingNotifications,
  IncomingRequests,
  IncomingNotifications
> {
  let nextRequestId = 1;
  let closed = false;
  const pending = new Map<RpcId, PendingRequest>();
  const requestHandlers = new Map<string, RequestHandler>();
  const notificationHandlers = new Map<string, NotificationHandler>();
  const unsubscribe = channel.subscribe(receive);
  const unsubscribeErrors = channel.subscribeErrors?.((error) => terminate(error));

  return {
    request(method, params, options) {
      if (closed) return Promise.reject(new Error("RPC peer is closed"));
      if (options?.signal?.aborted) return Promise.reject(abortError());
      const id = `rpc-${nextRequestId++}`;
      return new Promise((resolve, reject) => {
        pending.set(id, { resolve, reject });
        channel.send({
          jsonrpc: "2.0",
          id,
          method,
          params,
          ...(options?.meta === undefined ? {} : { meta: options.meta }),
        });
      }) as Promise<OutgoingRequests[typeof method]["result"]>;
    },
    notify(method, params) {
      if (closed) throw new Error("RPC peer is closed");
      channel.send({ jsonrpc: "2.0", method, params });
    },
    handleRequest(method, handler) {
      if (requestHandlers.has(method)) throw new Error(`RPC request handler already registered: ${method}`);
      requestHandlers.set(method, handler as RequestHandler);
      return () => requestHandlers.delete(method);
    },
    handleNotification(method, handler) {
      if (notificationHandlers.has(method)) {
        throw new Error(`RPC notification handler already registered: ${method}`);
      }
      notificationHandlers.set(method, handler as NotificationHandler);
      return () => notificationHandlers.delete(method);
    },
    close() {
      terminate(new Error("RPC peer is closed"));
    },
  };

  function terminate(error: unknown) {
    if (closed) return;
    closed = true;
    unsubscribe();
    unsubscribeErrors?.();
    for (const request of pending.values()) request.reject(error);
    pending.clear();
    requestHandlers.clear();
    notificationHandlers.clear();
  }

  function receive(message: RpcMessage) {
    if (closed || message.jsonrpc !== "2.0") return;
    if (!("method" in message)) {
      const request = pending.get(message.id);
      if (!request) return;
      pending.delete(message.id);
      if (message.error) request.reject(new RpcResponseError(message.error));
      else request.resolve(message.result);
      return;
    }
    if (!("id" in message)) {
      const handler = notificationHandlers.get(message.method);
      if (handler) void Promise.resolve(handler(message.params));
      return;
    }
    void handleIncomingRequest(message.id, message.method, message.params, message.scope);
  }

  async function handleIncomingRequest(id: RpcId, method: string, params: unknown, scope?: unknown) {
    const handler = requestHandlers.get(method);
    if (!handler) {
      channel.send({
        jsonrpc: "2.0",
        id,
        error: { code: -32601, message: `Method not found: ${method}` },
      });
      return;
    }
    try {
      const result = await handler(params, {
        requestId: id,
        scope,
        signal: new AbortController().signal,
      });
      channel.send({ jsonrpc: "2.0", id, result });
    } catch (error) {
      channel.send({
        jsonrpc: "2.0",
        id,
        error: {
          code: -32603,
          message: error instanceof Error ? error.message : "RPC request handler failed",
        },
      });
    }
  }
}

export class RpcResponseError extends Error {
  readonly name = "RpcResponseError";

  constructor(readonly responseError: unknown) {
    super(responseErrorMessage(responseError));
  }
}

function responseErrorMessage(error: unknown) {
  if (!error || typeof error !== "object") return "RPC request failed";
  const record = error as Record<string, unknown>;
  if (typeof record.message === "string") return record.message;
  const nested = record.error;
  if (nested && typeof nested === "object") {
    const message = (nested as Record<string, unknown>).message;
    if (typeof message === "string") return message;
  }
  return "RPC request failed";
}

function abortError() {
  const error = new Error("RPC request was cancelled");
  error.name = "AbortError";
  return error;
}
