import {
  SECRET_READ,
  SHELL_REVEAL_FILE,
  SHELL_SHOW_NOTIFICATION,
  type BackendConnection,
  type RequestId,
  type ServerRequestMethod,
  type ServerRequestResponseResultByMethod,
  type TypedServerRequest,
} from "@openaide/app-server-client";
import type { HostToWebviewMessage } from "@openaide/app-shell-contracts";
import type { PostHostMessage } from "../state/postHostMessage";

type ServerRequestConnection = Pick<BackendConnection, "serverRequests" | "respond">;

type ServerRequestBridgeOptions = {
  backendConnection: ServerRequestConnection;
  postHostMessage: PostHostMessage;
};

export function startAppServerServerRequestBridge({
  backendConnection,
  postHostMessage,
}: ServerRequestBridgeOptions) {
  const pending = new Map<string, ServerRequestMethod>();
  const stopServerRequests = backendConnection.serverRequests((request) => {
    if (!isShellHandledRequest(request)) return;
    pending.set(request.requestId, request.method);
    postHostMessage({
      type: "appServer.serverRequest",
      payload: {
        requestId: request.requestId,
        method: request.method,
        params: request.params,
      },
    });
  });

  return {
    handleHostMessage(message: HostToWebviewMessage) {
      if (message.type !== "appServer.serverRequest.result") return false;
      const requestId = message.payload.requestId;
      const method = pending.get(requestId);
      if (!method) return true;
      pending.delete(requestId);
      if (message.payload.method !== method) return true;
      const result = serverRequestResult(method, message.payload.result);
      if (!result.valid) return true;
      void respondToServerRequest(backendConnection, requestId, result.value);
      return true;
    },
    dispose() {
      pending.clear();
      stopServerRequests();
    },
  };
}

function isShellHandledRequest(request: TypedServerRequest<ServerRequestMethod>) {
  return (
    request.method === SECRET_READ
    || request.method === SHELL_SHOW_NOTIFICATION
    || request.method === SHELL_REVEAL_FILE
  );
}

async function respondToServerRequest(
  backendConnection: ServerRequestConnection,
  requestId: string,
  result: ServerRequestResponseResultByMethod[ServerRequestMethod],
) {
  await backendConnection.respond(
    requestId as RequestId,
    result,
  );
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
