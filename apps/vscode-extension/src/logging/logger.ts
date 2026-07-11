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

  info(message: string, fields: Record<string, unknown> = {}) {
    this.write("info", message, fields);
  }

  warn(message: string, fields: Record<string, unknown> = {}) {
    this.write("warn", message, fields);
  }

  error(message: string, fields: Record<string, unknown> = {}) {
    this.write("error", message, fields);
  }

  private write(level: string, message: string, fields: Record<string, unknown>) {
    const line = this.format(level, message, fields);
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

  private format(level: string, message: string, fields: Record<string, unknown>) {
    return JSON.stringify({
      timestamp: new Date().toISOString(),
      scope: this.scope,
      level,
      message,
      fields: redactFields(fields),
    });
  }
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
    redacted[key] = /prompt|secret|token|env|content|output|path|message|error/i.test(key)
      ? sanitizeDiagnosticText(value)
      : value;
  }
  return redacted;
}
