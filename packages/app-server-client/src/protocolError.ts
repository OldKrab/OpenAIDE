import type { ErrorEnvelope, ProtocolError, ResponseMeta } from "./generated/protocol.js";

export class AppServerProtocolError extends Error {
  readonly name = "AppServerProtocolError";
  readonly protocolError: ProtocolError;
  readonly meta?: ResponseMeta;

  constructor(readonly envelope: ErrorEnvelope) {
    super(envelope.error.message);
    this.protocolError = envelope.error;
    this.meta = envelope.meta;
  }
}

export function protocolErrorFromUnknown(error: unknown): Error {
  if (error instanceof AppServerProtocolError) return error;
  const envelope = errorEnvelopeFromUnknown(error);
  if (envelope) return new AppServerProtocolError(envelope);
  if (error instanceof Error) return error;
  if (typeof error === "string" && error.length > 0) return new Error(error);
  return new Error("App Server protocol request failed");
}

export function errorEnvelopeFromUnknown(value: unknown): ErrorEnvelope | undefined {
  if (!value || typeof value !== "object") return undefined;
  const record = value as Record<string, unknown>;
  const error = protocolErrorFromUnknownValue(record.error);
  if (!error) return undefined;
  return {
    error,
    meta: responseMetaFromUnknown(record.meta),
  };
}

function protocolErrorFromUnknownValue(value: unknown): ProtocolError | undefined {
  if (!value || typeof value !== "object") return undefined;
  const record = value as Record<string, unknown>;
  if (typeof record.code !== "string" || typeof record.message !== "string") {
    return undefined;
  }
  return {
    code: record.code as ProtocolError["code"],
    message: record.message,
    recoverable: typeof record.recoverable === "boolean" ? record.recoverable : undefined,
    target: errorTargetFromUnknown(record.target),
  };
}

function errorTargetFromUnknown(value: unknown): ProtocolError["target"] | undefined {
  if (value === null) return null;
  if (!value || typeof value !== "object") return undefined;
  const record = value as Record<string, unknown>;
  return {
    method: typeof record.method === "string" ? record.method : null,
    field: typeof record.field === "string" ? record.field : null,
  };
}

function responseMetaFromUnknown(value: unknown): ResponseMeta | undefined {
  if (!value || typeof value !== "object") return undefined;
  const record = value as Record<string, unknown>;
  return {
    clientRequestId: typeof record.clientRequestId === "string" ? record.clientRequestId as ResponseMeta["clientRequestId"] : undefined,
  };
}
