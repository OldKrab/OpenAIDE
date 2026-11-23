import { createInterface } from "node:readline";

// Pairing and reaching a computer cross the internet; this only bounds a hung process.
const DEFAULT_CALL_TIMEOUT_MS = 90_000;

/**
 * A request the gateway did not carry out, with a stable reason to log and
 * route on and a sentence for the user. The sentence is never logged.
 */
export class ComputerRefusal extends Error {
  constructor(reason, message) {
    super(message);
    this.name = "ComputerRefusal";
    this.reason = reason;
  }
}

/**
 * The Web App's link to its gateway process (ADR-0082): `openaide-app-server
 * gateway`, which holds the Web App's device key and paired computers and
 * starts one loopback endpoint per computer in use. The process is started on
 * first use and again on the next use after it exits. This module keeps only
 * what that process said: the list of computers, and the endpoint of each
 * computer some browser reached, dropped when the process reports the computer
 * refused this device or exits.
 */
export function createComputerGateway({ spawnGateway, logger, callTimeoutMs = DEFAULT_CALL_TIMEOUT_MS }) {
  let child;
  let ready;
  let stopping = false;
  let nextRequestId = 0;
  const pending = new Map();
  let computers;
  const connections = new Map();

  function start() {
    if (ready) return ready;
    const startedAt = Date.now();
    const process = spawnGateway();
    child = process;
    logger.info("computer_gateway_process_started", {
      process_id: typeof process.pid === "number" ? process.pid : undefined,
    });
    let markReady;
    let failStart;
    ready = new Promise((resolve, reject) => {
      markReady = resolve;
      failStart = reject;
    });
    // A caller that gave up still leaves the start pending; never report it unhandled.
    ready.catch(() => {});
    const ended = (fields) => {
      if (child !== process) return;
      child = undefined;
      ready = undefined;
      computers = undefined;
      connections.clear();
      const refusal = unavailable();
      failStart(refusal);
      for (const call of pending.values()) {
        clearTimeout(call.timer);
        call.reject(refusal);
      }
      pending.clear();
      const expected = stopping;
      logger[expected ? "info" : "warn"]("computer_gateway_process_exited", {
        expected,
        uptime_ms: Date.now() - startedAt,
        ...fields,
      });
    };
    process.once("exit", (code, signal) => ended({ exit_code: code, exit_signal: signal }));
    process.once("error", (error) => ended({ error_kind: errorKind(error) }));
    // Writing to a process that just died fails here, and its exit settles the calls.
    process.stdin.on("error", () => {});
    createInterface({ input: process.stdout }).on("line", (line) => {
      if (!line.trim()) return;
      let message;
      try {
        message = JSON.parse(line);
      } catch {
        logger.warn("computer_gateway_output_unreadable");
        return;
      }
      if (message?.event === "ready") {
        logger.info("computer_gateway_process_ready", { duration_ms: Date.now() - startedAt });
        markReady();
      } else if (message?.event === "failure" && typeof message.computer === "string") {
        // The next request for it asks again, and learns why once (ADR-0082).
        connections.delete(message.computer);
        logger.warn("computer_gateway_computer_refused", { reason: safeReason(message.reason) });
      } else if (typeof message?.id === "number" && pending.has(message.id)) {
        const call = pending.get(message.id);
        pending.delete(message.id);
        clearTimeout(call.timer);
        if (message.error) {
          call.reject(new ComputerRefusal(safeReason(message.error.reason), String(message.error.message ?? "")));
        } else {
          call.resolve(message.result);
        }
      } else {
        logger.warn("computer_gateway_output_unexpected");
      }
    });
    return ready;
  }

  async function call(method, fields = {}) {
    const requestId = ++nextRequestId;
    const startedAt = Date.now();
    logger.info("computer_gateway_call_started", { request_id: requestId, method });
    try {
      await start();
      // The process may have exited between its ready line and this turn.
      if (!child) throw unavailable();
      const result = await new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          pending.delete(requestId);
          reject(new ComputerRefusal("timeout", "The other computer took too long to answer. Try again."));
        }, callTimeoutMs);
        timer.unref?.();
        pending.set(requestId, { resolve, reject, timer });
        child.stdin.write(`${JSON.stringify({ id: requestId, method, ...fields })}\n`);
      });
      logger.info("computer_gateway_call_completed", {
        request_id: requestId,
        method,
        outcome: "ok",
        duration_ms: Date.now() - startedAt,
      });
      return result;
    } catch (error) {
      const refusal = error instanceof ComputerRefusal ? error : unavailable();
      logger.warn("computer_gateway_call_completed", {
        request_id: requestId,
        method,
        outcome: "refused",
        reason: refusal.reason,
        duration_ms: Date.now() - startedAt,
      });
      throw refusal;
    }
  }

  /**
   * The endpoint that carries requests to a paired computer, `{url, authToken}`.
   * The computer is reached before the first endpoint is handed out.
   */
  function connect(id) {
    let connecting = connections.get(id);
    if (!connecting) {
      connecting = call("connect", { computer: id }).then((connection) => ({
        url: new URL(connection.endpointUrl),
        authToken: connection.authToken,
      }));
      connections.set(id, connecting);
      connecting.catch(() => {
        if (connections.get(id) === connecting) connections.delete(id);
      });
    }
    return connecting;
  }

  return {
    /** The paired computers, `{id, name}`, in the order they were added. */
    async computers() {
      if (!computers) {
        const reading = call("computers").then((result) => result.computers);
        computers = reading;
        // A failed read is asked again next time.
        reading.catch(() => {
          if (computers === reading) computers = undefined;
        });
      }
      return computers;
    },

    /** Spends a Pairing Code; `inUse` is the computer whose own code is refused. */
    async pair(code, inUse) {
      const { computer } = await call("pair", { code, inUse: inUse ?? null });
      connections.delete(computer.id);
      computers = undefined;
      return computer;
    },

    connect,

    /**
     * Like `connect`, but asks the gateway even when an endpoint is kept: a browser
     * about to move to the computer must learn whether it answers now (ADR-0070).
     */
    reach(id) {
      connections.delete(id);
      return connect(id);
    },

    /** Drops an endpoint that stopped answering, so the next request asks for a new one. */
    release(id) {
      connections.delete(id);
    },

    async forget(id) {
      connections.delete(id);
      await call("forget", { computer: id });
      // A connection handed out while the gateway forgot the computer was stopped by it.
      connections.delete(id);
      computers = undefined;
    },

    /** Ends the process as the Web App leaves; it also ends when this process does. */
    stop(signal) {
      stopping = true;
      child?.kill(signal);
    },

    currentProcess: () => child,
  };
}

function unavailable() {
  return new ComputerRefusal("gateway_unavailable", "OpenAIDE could not reach other computers. Try again.");
}

/** Reasons come from the gateway; only a short stable word is logged or routed on. */
function safeReason(reason) {
  return typeof reason === "string" && /^[a-z_]{1,40}$/.test(reason) ? reason : "unknown";
}

function errorKind(error) {
  return error instanceof Error && error.name ? error.name : typeof error;
}
