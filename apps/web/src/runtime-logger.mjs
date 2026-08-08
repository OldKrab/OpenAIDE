/** Structured, metadata-only diagnostics for the Web App Shell runtime. */
export function createRuntimeLogger(scope = "openaide-web", sink = console) {
  return {
    info: (event, fields = {}) => write(sink.info, scope, "info", event, fields),
    warn: (event, fields = {}) => write(sink.warn, scope, "warn", event, fields),
    error: (event, fields = {}) => write(sink.error, scope, "error", event, fields),
  };
}

const WEBVIEW_FIELD_NAMES = new Set([
  "accepted",
  "after_sequence",
  "agent_id",
  "allow_server_change",
  "attempt",
  "body_bytes",
  "chat_items",
  "chunk_fallback",
  "connection_id",
  "duration_ms",
  "endpoint_changed",
  "endpoint_revision",
  "error_code",
  "error_kind",
  "error_name",
  "event",
  "failure_count",
  "frame_count",
  "generation",
  "generation_count",
  "has_client_request_id",
  "has_active_task",
  "http_status",
  "latest_snapshot_request_id",
  "last_server_sequence",
  "last_client_sequence",
  "method",
  "observed_replica_count",
  "operation_id",
  "next_server_id",
  "next_server_sequence",
  "project_id",
  "previous_status",
  "previous_server_id",
  "queue_depth",
  "queued_uploads",
  "recovery_in_progress",
  "replica_count",
  "reason",
  "response_code",
  "request",
  "request_id",
  "retry_delay_ms",
  "route",
  "session_list_request_id",
  "scope_key",
  "scope_kind",
  "server_id",
  "snapshot_intent",
  "snapshot_request_id",
  "status",
  "surface",
  "task_id",
  "task_status",
  "timeout_ms",
  "transport_operation_kind",
  "sequence",
  "expected_sequence",
  "first_available_sequence",
]);

/** Keeps Webview telemetry constrained to the contract's correlation metadata. */
export function safeWebviewTelemetryFields(payload) {
  if (!payload || typeof payload !== "object") return {};
  return Object.fromEntries(Object.entries(payload).filter(([key, value]) => {
    if (!WEBVIEW_FIELD_NAMES.has(key)) return false;
    return typeof value === "boolean"
      || typeof value === "number"
      || (typeof value === "string" && /^[a-zA-Z0-9_./,:*=-]{1,240}$/.test(value));
  }));
}

function write(output, scope, level, event, fields) {
  output(JSON.stringify({
    timestamp: new Date().toISOString(),
    scope,
    level,
    event,
    fields: redactFields(fields),
  }));
}

function redactFields(fields) {
  return Object.fromEntries(Object.entries(fields).map(([key, value]) => [
    key,
    isSensitiveField(key) ? "[redacted]" : value,
  ]));
}

function isSensitiveField(key) {
  if (/^error_name$/i.test(key)) return false;
  if (/_kind$|_code$|_count$|_bytes$|_status$/i.test(key)) return false;
  return /prompt|secret|token|password|credential|authorization|env|content|output|path|message|error|command|cwd|url/i.test(key);
}
