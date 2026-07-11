import { sanitizeDiagnosticText } from "../logging/logger";

export function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function webviewActionFields(message: Record<string, unknown>) {
  const payload = isObject(message.payload) ? message.payload : {};
  return omitUndefined({
    type: message.type,
    task_id: stringField(payload.task_id) ?? stringField(message.task_id),
    snapshot_request_id: numberField(message.snapshot_request_id),
    snapshot_intent: stringField(message.snapshot_intent),
    session_list_request_id: numberField(message.session_list_request_id),
    append: message.append === true ? true : undefined,
    archived: typeof payload.archived === "boolean" ? payload.archived : undefined,
    mode: stringField(payload.mode),
    agent_id: stringField(payload.agent_id) ?? stringField(payload.selected_agent_id),
    isolation: stringField(payload.selected_isolation),
    config_id: stringField(payload.config_id),
    has_workspace_root: typeof payload.workspace_root === "string" && payload.workspace_root.length > 0,
    has_prompt_text: typeof payload.prompt_text === "string" && payload.prompt_text.length > 0,
    attachment_count: Array.isArray(payload.context)
      ? payload.context.length
      : Array.isArray(payload.prompt_attachments)
        ? payload.prompt_attachments.length
        : undefined,
  });
}

export function webviewTelemetryFields(payload: Record<string, unknown>) {
  return omitUndefined({
    event: stringField(payload.event),
    surface: stringField(payload.surface),
    task_id: stringField(payload.task_id),
    snapshot_request_id: numberField(payload.snapshot_request_id),
    latest_snapshot_request_id: numberField(payload.latest_snapshot_request_id),
    snapshot_intent: stringField(payload.snapshot_intent),
    reason: stringField(payload.reason),
    request: stringField(payload.request),
    task_status: stringField(payload.task_status),
    chat_items: numberField(payload.chat_items),
    has_active_task: booleanField(payload.has_active_task),
    error_name: stringField(payload.error_name),
    error_message:
      typeof payload.error_message === "string" && payload.error_message.length > 0
        ? sanitizeDiagnosticText(payload.error_message)
        : undefined,
  });
}

function omitUndefined(fields: Record<string, unknown>) {
  return Object.fromEntries(Object.entries(fields).filter(([, value]) => value !== undefined));
}

function stringField(value: unknown) {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function numberField(value: unknown) {
  return typeof value === "number" ? value : undefined;
}

function booleanField(value: unknown) {
  return typeof value === "boolean" ? value : undefined;
}
