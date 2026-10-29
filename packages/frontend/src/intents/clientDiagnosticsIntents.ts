import {
  DIAGNOSTICS_RECORD_CLIENT_EVENTS,
  type BackendConnection,
  type ClientDiagnosticEvent,
  type ClientDiagnosticValue,
} from "@openaide/app-server-client";

type DiagnosticsConnection = Pick<BackendConnection, "request">;
export type ClientDiagnosticFields = Record<string, string | number | boolean | undefined>;

export type ClientDiagnosticsForwarder = {
  record(event: string, fields: ClientDiagnosticFields): void;
};

type ForwarderOptions = {
  /** Called once when the App Server refuses a batch; forwarding then stops for the connection. */
  onFailed(error: unknown): void;
  now?: () => number;
  schedule?: (flush: () => void, delayMs: number) => unknown;
};

/** A Task switch emits a few events at once; one request carries them together. */
const FLUSH_DELAY_MS = 250;
const MAX_BATCH = 32;

/**
 * Forwards the Frontend's metadata-only lifecycle events into the App Server log
 * (`diagnostics/recordClientEvents`), so a delay the user sees can be followed across the
 * Frontend and the App Server in one place. The App Shell's own telemetry sink stays the
 * primary record; this copy is best effort. An App Server older than the method, or any
 * failed batch, turns forwarding off for that connection rather than retrying.
 */
export function createClientDiagnosticsForwarder(
  connection: DiagnosticsConnection,
  { onFailed, now = Date.now, schedule = (flush, delayMs) => setTimeout(flush, delayMs) }: ForwarderOptions,
): ClientDiagnosticsForwarder {
  let queue: ClientDiagnosticEvent[] = [];
  let scheduled = false;
  let disabled = false;

  const flush = () => {
    scheduled = false;
    if (disabled || queue.length === 0) return;
    const events = queue;
    queue = [];
    // A synchronous throw from the connection is a refused batch like a rejection.
    Promise.resolve().then(() => connection.request(DIAGNOSTICS_RECORD_CLIENT_EVENTS, { events })).catch((error: unknown) => {
      if (disabled) return;
      disabled = true;
      queue = [];
      onFailed(error);
    });
  };

  return {
    record(event, fields) {
      if (disabled) return;
      queue.push({ event, atMs: now(), fields: protocolFields(fields) });
      if (queue.length >= MAX_BATCH) flush();
      else if (!scheduled) {
        scheduled = true;
        schedule(flush, FLUSH_DELAY_MS);
      }
    },
  };
}

/**
 * The App Server accepts whole numbers, flags and single words. Its log redacts keys that
 * name an error unless they end in `_kind` or `_code`, so `error_name` travels as `error_kind`.
 */
export function protocolFields(fields: ClientDiagnosticFields): Record<string, ClientDiagnosticValue> {
  const result: Record<string, ClientDiagnosticValue> = {};
  for (const [key, value] of Object.entries(fields)) {
    if (value === undefined) continue;
    if (typeof value === "number" && !Number.isFinite(value)) continue;
    result[key === "error_name" ? "error_kind" : key] = typeof value === "number" ? Math.round(value) : value;
  }
  return result;
}

const forwarders = new WeakMap<DiagnosticsConnection, ClientDiagnosticsForwarder>();

/** The connection's forwarder, created on first use so every caller shares one batch. */
export function clientDiagnosticsFor(
  connection: DiagnosticsConnection,
  onFailed: (error: unknown) => void,
): ClientDiagnosticsForwarder {
  let forwarder = forwarders.get(connection);
  if (!forwarder) {
    forwarder = createClientDiagnosticsForwarder(connection, { onFailed });
    forwarders.set(connection, forwarder);
  }
  return forwarder;
}
