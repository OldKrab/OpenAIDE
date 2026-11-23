import type {
  ComputerInUse,
  ComputerSwitchResult,
  ComputersCapability,
  ComputersSnapshot,
  ShellComputer,
} from "../../../packages/frontend/src/services/computers";
import { forgetClientInstanceId } from "../../../packages/frontend/src/services/backendInitialization";

const ROUTES = "/__openaide-computers";
const READ_FAILED = "OpenAIDE could not read its paired computers. Try again.";
const NO_ANSWER = "The Web App's server did not answer. Try again.";
const REFUSED = "OpenAIDE could not do that. Try again.";
const MAX_MESSAGE_LENGTH = 400;

type Action = "list" | "use" | "pair" | "forget";

export type WebComputersEnvironment = {
  /** The computer this page opened on (`data-computer-id`); undefined on the Web App's own. */
  pinned: string | undefined;
  fetch: typeof fetch;
  /** Loads the page again; the server opens it on the browser's choice. */
  reload(): void;
  log(level: "info" | "warn", event: string, fields: Record<string, unknown>): void;
};

/** The switch ended before the Web App's server answered, so the browser's choice is unchanged. */
class Cancelled extends Error {}

/**
 * The Web App's paired computers (ADR-0082). The Web App's server keeps the list
 * and the browser's choice; this page only asks it. A page stays on the computer
 * it opened on, so a switch asks the server to reach the chosen computer and keep
 * it as the browser's choice, then reloads this tab onto it as a new client
 * (ADR-0070). Other tabs move when they next load.
 */
export function createWebComputers(environment: WebComputersEnvironment): ComputersCapability {
  const { pinned, log } = environment;
  let snapshot: ComputersSnapshot | undefined;
  let reading: Promise<void> | undefined;
  let switching: AbortController | undefined;
  const listeners = new Set<() => void>();
  const publish = (next: ComputersSnapshot) => {
    if (snapshot && JSON.stringify(snapshot) === JSON.stringify(next)) return;
    snapshot = next;
    listeners.forEach((listener) => listener());
  };
  const read = () => reading ??= (async () => {
    try {
      publish({ device: "computer", computers: rows(pinned, pairedComputers(await call("list"))) });
    } catch {
      publish({ device: "computer", computers: snapshot?.computers ?? rows(pinned, []), error: READ_FAILED });
    } finally {
      reading = undefined;
    }
  })();

  /**
   * Answers with the result, or throws an `Error` whose message is a sentence for
   * the user, or `Cancelled` when `signal` ended it before the server answered.
   */
  async function call(action: Action, body?: Record<string, unknown>, operationId?: string, signal?: AbortSignal): Promise<unknown> {
    const startedAt = performance.now();
    const done = (level: "info" | "warn", fields: Record<string, unknown>) => log(level, "web_computers_call_completed", {
      action,
      operation_id: operationId,
      ...fields,
      duration_ms: Math.round(performance.now() - startedAt),
    });
    log("info", "web_computers_call_started", { action, operation_id: operationId });
    let response: Response;
    try {
      response = await environment.fetch(`${ROUTES}/${action}`, body ? {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
        credentials: "same-origin",
        cache: "no-store",
        signal,
      } : { credentials: "same-origin", cache: "no-store" });
    } catch {
      if (signal?.aborted) {
        done("info", { outcome: "cancelled" });
        throw new Cancelled();
      }
      done("warn", { outcome: "unreachable" });
      throw new Error(NO_ANSWER);
    }
    // Once the server answered, a cancel is too late: its answer already set the browser's choice.
    const answer = await response.json().catch(() => undefined) as { error?: { reason?: unknown; message?: unknown } } | undefined;
    if (response.ok) {
      done("info", { outcome: "ok", status: response.status });
      return answer;
    }
    const reason = typeof answer?.error?.reason === "string" && /^[a-z_]{1,40}$/.test(answer.error.reason) ? answer.error.reason : "unknown";
    done("warn", { outcome: "refused", status: response.status, reason });
    const message = answer?.error?.message;
    throw new Error(typeof message === "string" && message && message.length <= MAX_MESSAGE_LENGTH ? message : REFUSED);
  }

  /** Replaces this page with one on the browser's new choice, as a new client of it. */
  function leave(action: Action, operationId: string | undefined) {
    log("info", "web_computer_switch_page_reload", { action, operation_id: operationId });
    // The App Server left keeps this page's client until its reconnect grace ends. Unlike the
    // Desktop's, a Web App's App Server has no update a lingering client could hold up, so the
    // page leaves without detaching.
    forgetClientInstanceId();
    environment.reload();
  }

  void read();
  return {
    snapshot: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      // Another tab may have paired or forgotten a computer since, so a new watcher, such as
      // Settings opening, reads the list again; the server answers from memory.
      void read();
      return () => listeners.delete(listener);
    },
    async use(id, operationId): Promise<ComputerSwitchResult> {
      if (id === pinned) return "unchanged";
      const controller = new AbortController();
      switching = controller;
      try {
        await call("use", { computer: id ?? null, operationId }, operationId, controller.signal);
      } catch (error) {
        if (error instanceof Cancelled) return "cancelled";
        throw error;
      } finally {
        if (switching === controller) switching = undefined;
      }
      leave("use", operationId);
      return "switched";
    },
    cancel: async () => {
      switching?.abort();
    },
    async forget(id) {
      await call("forget", { computer: id });
      if (id !== pinned) {
        await read();
        return;
      }
      // Forgetting the computer in use moves this tab to the Web App's own computer, even when
      // another tab chose a different one since.
      await call("use", { computer: null });
      leave("forget", undefined);
    },
    async pair(code: string, inUse?: ComputerInUse) {
      try {
        await call("pair", { code, inUse: inUse ?? null });
      } catch (error) {
        // A computer paired but not reached now stays in the list, as on the Desktop.
        await read();
        throw error;
      }
      leave("pair", undefined);
    },
  };
}

/** The list with the computer this page opened on marked in use. */
function rows(pinned: string | undefined, paired: { id: string; name: string }[]): ShellComputer[] {
  const computers = paired.map(({ id, name }) => ({ id, name, current: id === pinned }));
  // Forgotten in another tab, the computer this page opened on is still the one it uses until
  // it reloads; listing it keeps the switcher's way back to the Web App's own computer.
  if (pinned && !computers.some((computer) => computer.current)) computers.push({ id: pinned, name: "", current: true });
  return computers;
}

function pairedComputers(answer: unknown): { id: string; name: string }[] {
  const computers = (answer as { computers?: unknown } | undefined)?.computers;
  if (!Array.isArray(computers)) throw new Error(READ_FAILED);
  return computers.flatMap((computer: { id?: unknown; name?: unknown }) => typeof computer?.id === "string"
    ? [{ id: computer.id, name: typeof computer.name === "string" ? computer.name : "" }]
    : []);
}
