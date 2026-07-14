/** Structured, metadata-only diagnostics for the Web App Shell runtime. */
export function createRuntimeLogger(scope = "openaide-web", sink = console) {
  return {
    info: (event, fields = {}) => write(sink.info, scope, "info", event, fields),
    warn: (event, fields = {}) => write(sink.warn, scope, "warn", event, fields),
    error: (event, fields = {}) => write(sink.error, scope, "error", event, fields),
  };
}

const WEBVIEW_FIELD_NAMES = new Set([
  "agent_id",
  "chat_items",
  "error_code",
  "error_name",
  "event",
  "has_active_task",
  "latest_snapshot_request_id",
  "project_id",
  "reason",
  "request",
  "session_list_request_id",
  "snapshot_intent",
  "snapshot_request_id",
  "surface",
  "task_id",
  "task_status",
]);

/** Keeps Webview telemetry constrained to the contract's correlation metadata. */
export function safeWebviewTelemetryFields(payload) {
  if (!payload || typeof payload !== "object") return {};
  return Object.fromEntries(Object.entries(payload).filter(([key, value]) => {
    if (!WEBVIEW_FIELD_NAMES.has(key)) return false;
    return typeof value === "boolean"
      || typeof value === "number"
      || (typeof value === "string" && /^[a-zA-Z0-9_.:-]{1,160}$/.test(value));
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
  return /prompt|secret|token|password|env|content|output|path|message|error|command|cwd|url/i.test(key);
}
