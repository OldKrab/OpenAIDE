import {
  AppServerProtocolError,
  errorEnvelopeFromUnknown,
} from "./protocolError.js";
import type {
  AppServerEvent,
  ErrorEnvelope,
  PendingRequestScope,
  ServerRequestMethod,
  ServerRequestParamsByMethod,
  TypedServerRequest,
} from "./generated/protocol.js";
import {
  PERMISSION_REQUEST,
  QUESTION_REQUEST,
  SECRET_READ,
  SHELL_REVEAL_FILE,
  SHELL_SHOW_NOTIFICATION,
} from "./generated/protocol.js";

export type JsonRpcId = string | number;

export type LocalHttpWireMessage =
  | { kind: "response"; id: JsonRpcId; result?: unknown; error?: ErrorEnvelope }
  | { kind: "event"; event: AppServerEvent }
  | { kind: "serverRequest"; request: TypedServerRequest<ServerRequestMethod> }
  | { kind: "ignored" };

export function parseLocalHttpWireMessages(body: string): LocalHttpWireMessage[] {
  const parsed = JSON.parse(body) as unknown;
  const values = Array.isArray(parsed) ? parsed : [parsed];
  return values.map(parseWireMessage);
}

export function responseResultForId(messages: LocalHttpWireMessage[], id: JsonRpcId): unknown {
  const response = messages.find((message) => message.kind === "response" && message.id === id);
  if (!response || response.kind !== "response") {
    throw new Error("App Server response did not include the request id");
  }
  if (response.error) {
    throw new AppServerProtocolError(response.error);
  }
  return unwrapResponseEnvelope(response.result);
}

function parseWireMessage(value: unknown): LocalHttpWireMessage {
  if (!value || typeof value !== "object") return { kind: "ignored" };
  const record = value as Record<string, unknown>;
  if (record.jsonrpc !== "2.0") return { kind: "ignored" };

  const error = errorEnvelopeFromUnknown(record.error);
  if (isJsonRpcId(record.id) && (record.result !== undefined || error)) {
    return { kind: "response", id: record.id, result: record.result, error };
  }
  if (record.method === "app/event" && record.params) {
    return { kind: "event", event: record.params as AppServerEvent };
  }
  if (typeof record.method === "string" && typeof record.id === "string") {
    const request = typedServerRequest(record.id, record.scope, record.method, record.params ?? {});
    return {
      kind: "serverRequest",
      request,
    };
  }
  return { kind: "ignored" };
}

function typedServerRequest(
  requestId: string,
  scope: unknown,
  method: string,
  params: unknown,
): TypedServerRequest<ServerRequestMethod> {
  if (!isServerRequestMethod(method)) {
    throw new Error(`Unsupported App Server server request method: ${method}`);
  }
  if (!isPendingRequestScope(scope)) {
    throw new Error("App Server server request is missing a valid scope");
  }
  validateServerRequestParams(method, params);
  return {
    requestId: requestId as TypedServerRequest<ServerRequestMethod>["requestId"],
    scope,
    method,
    params: params as ServerRequestParamsByMethod[ServerRequestMethod],
  };
}

function isPendingRequestScope(value: unknown): value is PendingRequestScope {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  if (record.kind === "task") return typeof record.taskId === "string";
  if (record.kind === "client") return typeof record.clientInstanceId === "string";
  return false;
}

function isServerRequestMethod(method: string): method is ServerRequestMethod {
  return (
    method === PERMISSION_REQUEST ||
    method === QUESTION_REQUEST ||
    method === SECRET_READ ||
    method === SHELL_SHOW_NOTIFICATION ||
    method === SHELL_REVEAL_FILE
  );
}

function validateServerRequestParams(method: ServerRequestMethod, params: unknown) {
  const object = recordParams(method, params);
  if (method === PERMISSION_REQUEST) {
    requiredString(method, object, "title");
    const toolCall = recordParams(method, object.toolCall, "toolCall");
    requiredString(method, toolCall, "id");
    requiredString(method, toolCall, "title");
    const options = object.options;
    if (!Array.isArray(options)) throw new Error(`${method} params.options must be an array`);
    for (const [index, option] of options.entries()) {
      const item = recordParams(method, option, `options[${index}]`);
      requiredString(method, item, "optionId", `options[${index}].optionId`);
      requiredString(method, item, "name", `options[${index}].name`);
      requiredString(method, item, "kind", `options[${index}].kind`);
    }
    return;
  }
  if (method === QUESTION_REQUEST) {
    requiredString(method, object, "message");
    const fields = object.fields;
    if (!Array.isArray(fields)) throw new Error(`${method} params.fields must be an array`);
    for (const [index, field] of fields.entries()) {
      const item = recordParams(method, field, `fields[${index}]`);
      const kind = requiredString(method, item, "kind", `fields[${index}].kind`);
      requiredString(method, item, "key", `fields[${index}].key`);
      requiredString(method, item, "title", `fields[${index}].title`);
      if (typeof item.required !== "boolean") {
        throw new Error(`${method} params.fields[${index}].required must be a boolean`);
      }
      if (kind === "singleSelect" || kind === "multiSelect") {
        const options = item.options;
        if (!Array.isArray(options)) {
          throw new Error(`${method} params.fields[${index}].options must be an array`);
        }
        for (const [optionIndex, option] of options.entries()) {
          const choice = recordParams(method, option, `fields[${index}].options[${optionIndex}]`);
          requiredString(method, choice, "value", `fields[${index}].options[${optionIndex}].value`);
          requiredString(method, choice, "label", `fields[${index}].options[${optionIndex}].label`);
        }
      }
    }
    return;
  }
  if (method === SECRET_READ) {
    requiredString(method, object, "key");
    return;
  }
  if (method === SHELL_SHOW_NOTIFICATION) {
    const level = requiredString(method, object, "level");
    if (level !== "info" && level !== "warning" && level !== "error") {
      throw new Error(`${method} params.level must be a shell notification level`);
    }
    requiredString(method, object, "message");
    if (object.actions !== undefined) {
      if (!Array.isArray(object.actions)) throw new Error(`${method} params.actions must be an array`);
      for (const [index, action] of object.actions.entries()) {
        const item = recordParams(method, action, `actions[${index}]`);
        requiredString(method, item, "actionId", `actions[${index}].actionId`);
        requiredString(method, item, "label", `actions[${index}].label`);
      }
    }
    return;
  }
  if (method === SHELL_REVEAL_FILE) {
    requiredString(method, object, "fileHandleId");
  }
}

function recordParams(method: string, value: unknown, field = "params"): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${method} ${formatParamField(field)} must be an object`);
  }
  return value as Record<string, unknown>;
}

function formatParamField(field: string) {
  return field === "params" ? "params" : `params.${field}`;
}

function requiredString(
  method: string,
  object: Record<string, unknown>,
  field: string,
  label = field,
) {
  const value = object[field];
  if (typeof value !== "string") throw new Error(`${method} params.${label} must be a string`);
  return value;
}

function unwrapResponseEnvelope(value: unknown): unknown {
  if (!value || typeof value !== "object") return value;
  const record = value as Record<string, unknown>;
  return Object.prototype.hasOwnProperty.call(record, "result") ? record.result : value;
}

function isJsonRpcId(value: unknown): value is JsonRpcId {
  return typeof value === "string" || typeof value === "number";
}
