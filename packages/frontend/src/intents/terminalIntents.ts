import {
  AppServerProtocolError,
  TERMINALS_ATTACH,
  TERMINALS_CLOSE,
  TERMINALS_OPEN,
  type BackendConnection,
  type ClientRequestId,
  type ProtocolErrorCode,
  type TerminalAttach,
  type TerminalTarget,
  type TerminalsOpenResult,
} from "@openaide/app-server-client";

type TerminalConnection = Pick<BackendConnection, "request">;

/** Receives metadata-only diagnostics: ids, outcomes and durations, never shell content. */
export type TerminalIntentLog = (level: "info" | "warn", event: string, fields: Record<string, string | number>) => void;

export type TerminalCloseOutcome = "closed" | "busy" | "gone";

/**
 * Terminal lifecycle intents (ADR-0071). App Server owns every shell, so these only ask; the
 * resulting list arrives through the Terminals subscription. Bytes move on the socket that an
 * attach token opens, see `services/terminalSockets.ts`.
 */
export type TerminalIntents = {
  /** Starts a shell in the Project root or the Task Workspace and returns its first attach. */
  open(target: TerminalTarget, size: { cols: number; rows: number }): Promise<TerminalsOpenResult>;
  /** A fresh single-use token for a Terminal this client opened earlier or another client opened. */
  attach(terminalId: string): Promise<TerminalAttach>;
  /** `busy` when a program is running and `force` is false, so the caller can ask the user. */
  close(terminalId: string, force: boolean): Promise<TerminalCloseOutcome>;
};

export function createTerminalIntents(
  connection: TerminalConnection,
  log: TerminalIntentLog = consoleLog,
): TerminalIntents {
  async function run<T>(operation: string, terminalId: string | undefined, request: (id: ClientRequestId) => Promise<T>) {
    const operationId = crypto.randomUUID();
    const started = performance.now();
    const fields = { operation, operation_id: operationId, terminal_id: terminalId ?? "none", attempt: 1 };
    log("info", "terminal_intent_started", fields);
    try {
      const result = await request(operationId as ClientRequestId);
      log("info", "terminal_intent_completed", { ...fields, outcome: "success", duration_ms: elapsed(started) });
      return result;
    } catch (error) {
      log("warn", "terminal_intent_completed", {
        ...fields,
        outcome: "failure",
        error_kind: errorCode(error) ?? "transport",
        duration_ms: elapsed(started),
      });
      throw error;
    }
  }
  return {
    open: (target, { cols, rows }) =>
      run("open", undefined, (id) => connection.request(TERMINALS_OPEN, { target, cols, rows }, { clientRequestId: id })),
    attach: async (terminalId) =>
      (await run("attach", terminalId, (id) => connection.request(TERMINALS_ATTACH, { terminalId }, { clientRequestId: id }))).attach,
    close: async (terminalId, force) => {
      try {
        await run("close", terminalId, (id) => connection.request(TERMINALS_CLOSE, { terminalId, force }, { clientRequestId: id }));
        return "closed";
      } catch (error) {
        const code = errorCode(error);
        if (code === "conflict") return "busy";
        if (code === "notFound") return "gone";
        throw error;
      }
    },
  };
}

function errorCode(error: unknown): ProtocolErrorCode | undefined {
  return error instanceof AppServerProtocolError ? error.protocolError.code : undefined;
}

function elapsed(started: number) {
  return Math.round(performance.now() - started);
}

function consoleLog(level: "info" | "warn", event: string, fields: Record<string, string | number>) {
  const line = [event, ...Object.entries(fields).map(([key, value]) => `${key}=${value}`)].join(" ");
  if (level === "warn") console.warn(line);
  else console.info(line);
}
