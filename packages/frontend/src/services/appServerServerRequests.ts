import {
  SECRET_READ,
  SHELL_REVEAL_FILE,
  SHELL_SHOW_NOTIFICATION,
  type BackendConnection,
  type BackendRequestContext,
  type ServerRequestMethod,
  type ServerRequestParamsByMethod,
  type ServerRequestResponseResultByMethod,
} from "@openaide/app-server-client";
import type { HostToWebviewMessage } from "@openaide/app-shell-contracts";
import type { PostHostMessage } from "../state/postHostMessage";

type ServerRequestConnection = Pick<BackendConnection, "handleRequest">;

type ServerRequestBridgeOptions = {
  backendConnection: ServerRequestConnection;
  postHostMessage: PostHostMessage;
};

export function startAppServerServerRequestBridge({
  backendConnection,
  postHostMessage,
}: ServerRequestBridgeOptions) {
  const pending = new Map<string, {
    method: ServerRequestMethod;
    resolve(result: ServerRequestResponseResultByMethod[ServerRequestMethod]): void;
  }>();
  const stops = shellHandledMethods().map((method) => backendConnection.handleRequest(
    method,
    (params, context) => forwardRequest(method, params, context) as never,
  ));

  function forwardRequest<M extends ServerRequestMethod>(
    method: M,
    params: ServerRequestParamsByMethod[M],
    context: BackendRequestContext,
  ) {
    const result = new Promise<ServerRequestResponseResultByMethod[M]>((resolve) => {
      pending.set(context.requestId, { method, resolve: resolve as never });
    });
    postHostMessage({
      type: "appServer.serverRequest",
      payload: {
        requestId: context.requestId,
        method,
        params,
      },
    });
    return result;
  }

  return {
    handleHostMessage(message: HostToWebviewMessage) {
      if (message.type !== "appServer.serverRequest.result") return false;
      const requestId = message.payload.requestId;
      const request = pending.get(requestId);
      if (!request) return true;
      pending.delete(requestId);
      if (message.payload.method !== request.method) return true;
      const result = serverRequestResult(request.method, message.payload.result);
      if (!result.valid) return true;
      request.resolve(result.value);
      return true;
    },
    dispose() {
      pending.clear();
      for (const stop of stops) stop();
    },
  };
}

function shellHandledMethods() {
  return [SECRET_READ, SHELL_SHOW_NOTIFICATION, SHELL_REVEAL_FILE] as const;
}

function serverRequestResult(
  method: ServerRequestMethod,
  result: unknown,
): { valid: true; value: ServerRequestResponseResultByMethod[ServerRequestMethod] } | { valid: false } {
  if (!isRecord(result)) return { valid: false };
  if (method === SECRET_READ) {
    return result.value === null || typeof result.value === "string"
      ? { valid: true, value: { value: result.value } }
      : { valid: false };
  }
  if (method === SHELL_SHOW_NOTIFICATION) {
    return result.actionId === null || typeof result.actionId === "string"
      ? { valid: true, value: { actionId: result.actionId } }
      : { valid: false };
  }
  if (method === SHELL_REVEAL_FILE) {
    return typeof result.revealed === "boolean"
      ? { valid: true, value: { revealed: result.revealed } }
      : { valid: false };
  }
  return { valid: false };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object";
}
