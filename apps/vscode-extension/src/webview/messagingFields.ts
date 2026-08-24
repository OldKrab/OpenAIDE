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
    error_code: stringField(payload.error_code),
    agent_id: stringField(payload.agent_id),
    project_id: stringField(payload.project_id),
    selection_source: stringField(payload.selection_source),
    client_identity_source: stringField(payload.client_identity_source),
    shell_project_present: booleanField(payload.shell_project_present),
    shell_project_valid: booleanField(payload.shell_project_valid),
    retained_project_present: booleanField(payload.retained_project_present),
    retained_project_valid: booleanField(payload.retained_project_valid),
    default_project_present: booleanField(payload.default_project_present),
    default_project_valid: booleanField(payload.default_project_valid),
    workspace_roots_seeded: booleanField(payload.workspace_roots_seeded),
    session_list_request_id: numberField(payload.session_list_request_id),
    operation_id: stringField(payload.operation_id),
    attempt: numberField(payload.attempt),
    duration_ms: numberField(payload.duration_ms),
    queue_depth: numberField(payload.queue_depth),
    source_length: numberField(payload.source_length),
    output_bytes: numberField(payload.output_bytes),
    outcome: stringField(payload.outcome),
    cache_hit: booleanField(payload.cache_hit),
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
