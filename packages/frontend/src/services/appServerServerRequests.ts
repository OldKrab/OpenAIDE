import {
  PERMISSION_REQUEST,
  QUESTION_REQUEST,
  SECRET_READ,
  SHELL_REVEAL_FILE,
  SHELL_SHOW_NOTIFICATION,
  type BackendConnection,
  type PermissionRequestOptionKind,
  type PermissionRequestParams,
  type QuestionRequestParams,
  type RequestId,
  type ServerRequestMethod,
  type ServerRequestResponseResultByMethod,
  type TypedServerRequest,
} from "@openaide/app-server-client";
import type { ChatMessage, HostToWebviewMessage, PermissionOptionKind, WebviewToHostMessage } from "@openaide/app-shell-contracts";
import { mapPendingProtocolQuestion } from "../state/questionProtocolMapping";

type ServerRequestConnection = Pick<BackendConnection, "serverRequests" | "respond">;

type ServerRequestBridgeOptions = {
  backendConnection: ServerRequestConnection;
  onPermissionRequest?: (requestId: string, message: ChatMessage, taskId?: string) => void;
  onQuestionRequest?: (requestId: string, message: ChatMessage, taskId?: string) => void;
  postHostMessage: (message: WebviewToHostMessage) => void;
};

export function startAppServerServerRequestBridge({
  backendConnection,
  onPermissionRequest,
  onQuestionRequest,
  postHostMessage,
}: ServerRequestBridgeOptions) {
  const pending = new Map<string, ServerRequestMethod>();
  const stopServerRequests = backendConnection.serverRequests((request) => {
    if (request.method === PERMISSION_REQUEST) {
      onPermissionRequest?.(
        request.requestId,
        permissionMessageFromServerRequest(request as TypedServerRequest<typeof PERMISSION_REQUEST>),
        taskIdFromServerRequest(request),
      );
      return;
    }
    if (request.method === QUESTION_REQUEST) {
      onQuestionRequest?.(
        request.requestId,
        questionMessageFromServerRequest(request as TypedServerRequest<typeof QUESTION_REQUEST>),
        taskIdFromServerRequest(request),
      );
      return;
    }
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

function questionMessageFromServerRequest(
  request: TypedServerRequest<typeof QUESTION_REQUEST>,
): ChatMessage {
  const messageId = `app-server-question-${request.requestId}`;
  const params: QuestionRequestParams = request.params;
  return {
    cursor: messageId,
    identity: messageId,
    message_id: messageId,
    message_type: "elicitation",
    message: {
      ...mapPendingProtocolQuestion(request.requestId, params, new Date().toISOString()),
      id: messageId,
    },
  };
}

function taskIdFromServerRequest(request: TypedServerRequest<ServerRequestMethod>) {
  return request.scope.kind === "task" ? request.scope.taskId : undefined;
}

function permissionMessageFromServerRequest(
  request: TypedServerRequest<typeof PERMISSION_REQUEST>,
): ChatMessage {
  const messageId = `app-server-permission-${request.requestId}`;
  const params: PermissionRequestParams = request.params;
  return {
    cursor: messageId,
    identity: messageId,
    message_id: messageId,
    message_type: "permission",
    message: {
      kind: "permission",
      id: messageId,
      request_id: request.requestId,
      app_server_request_id: request.requestId,
      title: params.title,
      description: params.description ?? undefined,
      scope: params.scope ?? undefined,
      risk: params.risk ?? undefined,
      tool_call: {
        id: params.toolCall.id,
        title: params.toolCall.title,
        kind: params.toolCall.kind ?? undefined,
      },
      state: "pending",
      created_at: new Date().toISOString(),
      options: params.options.map((option) => ({
        id: option.optionId,
        label: option.name,
        kind: permissionOptionKind(option.kind),
      })),
    },
  };
}

function permissionOptionKind(kind: PermissionRequestOptionKind): PermissionOptionKind {
  if (kind === "allowOnce" || kind === "allowAlways") return "allow";
  if (kind === "rejectOnce" || kind === "rejectAlways") return "deny";
  return "other";
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
  if (method === PERMISSION_REQUEST) {
    return typeof result.optionId === "string"
      ? { valid: true, value: { optionId: result.optionId } }
      : { valid: false };
  }
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
