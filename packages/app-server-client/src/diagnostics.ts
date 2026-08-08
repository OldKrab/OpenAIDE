/** Metadata-only diagnostics shared by transport and logical-session layers. */
export type DiagnosticsLogger = {
  info(event: string, fields?: Record<string, unknown>): void;
  warn(event: string, fields?: Record<string, unknown>): void;
  error(event: string, fields?: Record<string, unknown>): void;
};

/** Creates a console logger for shells that do not provide a persistent sink. */
export function createDiagnosticsLogger(
  scope = "openaide-app-server-client",
  sink: Pick<Console, "info" | "warn" | "error"> = console,
): DiagnosticsLogger {
  return {
    info: (event, fields = {}) => write(sink.info, scope, "info", event, fields),
    warn: (event, fields = {}) => write(sink.warn, scope, "warn", event, fields),
    error: (event, fields = {}) => write(sink.error, scope, "error", event, fields),
  };
}

function write(
  output: (...values: unknown[]) => void,
  scope: string,
  level: string,
  event: string,
  fields: Record<string, unknown>,
) {
  output(JSON.stringify({
    timestamp: new Date().toISOString(),
    scope,
    level,
    event,
    fields: redactFields(fields),
  }));
}

function redactFields(fields: Record<string, unknown>) {
  return Object.fromEntries(Object.entries(fields).map(([key, value]) => [
    key,
    isSensitiveField(key) ? "[redacted]" : value,
  ]));
}

function isSensitiveField(key: string) {
  if (/^error_name$/i.test(key)) return false;
  if (/_kind$|_code$|_count$|_bytes$|_status$/i.test(key)) return false;
  return /prompt|secret|token|password|credential|authorization|env|content|output|path|message|error|command|cwd|url/i.test(key);
}
