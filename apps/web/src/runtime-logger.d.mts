export type RuntimeLogger = {
  info(event: string, fields?: Record<string, unknown>): void;
  warn(event: string, fields?: Record<string, unknown>): void;
  error(event: string, fields?: Record<string, unknown>): void;
};

export function createRuntimeLogger(scope?: string, sink?: Console): RuntimeLogger;
export function safeWebviewTelemetryFields(payload: unknown): Record<string, unknown>;
