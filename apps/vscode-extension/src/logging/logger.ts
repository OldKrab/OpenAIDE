import * as fs from "node:fs/promises";
import * as path from "node:path";

export class ExtensionLogger {
  private fileWrite: Promise<unknown> = Promise.resolve();

  constructor(
    private readonly scope: string,
    private logFile?: string,
  ) {}

  setLogFile(logFile: string) {
    this.logFile = logFile;
  }

  info(event: string, fields: Record<string, unknown> = {}) {
    this.write("info", event, fields);
  }

  warn(event: string, fields: Record<string, unknown> = {}) {
    this.write("warn", event, fields);
  }

  error(event: string, fields: Record<string, unknown> = {}) {
    this.write("error", event, fields);
  }

  private write(level: string, event: string, fields: Record<string, unknown>) {
    const line = this.format(level, event, fields);
    if (level === "error") console.error(line);
    else if (level === "warn") console.warn(line);
    else console.info(line);
    if (!this.logFile) return;
    this.fileWrite = this.fileWrite
      .then(async () => {
        await fs.mkdir(path.dirname(this.logFile!), { recursive: true });
        await fs.appendFile(this.logFile!, `${line}\n`, "utf8");
      })
      .catch((error) => {
        console.warn(this.format("warn", "failed to write extension log file", { error: String(error) }));
      });
  }

  private format(level: string, event: string, fields: Record<string, unknown>) {
    return JSON.stringify({
      timestamp: new Date().toISOString(),
      scope: this.scope,
      level,
      event: normalizeEventName(event),
      fields: redactFields(fields),
    });
  }
}

function normalizeEventName(event: string) {
  return event
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "") || "invalid_event";
}

export function sanitizeDiagnosticText(value: unknown) {
  const text = value instanceof Error ? value.message : String(value);
  return text
    .replace(/[A-Za-z]:[\\/][^\s"'<>]+/g, "[path]")
    .replace(/\/[^\s"'<>]+/g, "[path]")
    .replace(/\\[^\s"'<>]+/g, "[path]")
    .replace(/\b(prompt|secret|token|password|content|output)\b/gi, "[redacted]");
}

function redactFields(fields: Record<string, unknown>) {
  const redacted: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(fields)) {
    redacted[key] = isSensitiveField(key) ? "[redacted]" : value;
  }
  return redacted;
}

function isSensitiveField(key: string) {
  if (/^error_name$/i.test(key)) return false;
  if (/_kind$|_code$|_count$|_bytes$|_status$/i.test(key)) return false;
  return /prompt|secret|token|password|env|content|output|path|message|error|command|cwd/i.test(key);
}
