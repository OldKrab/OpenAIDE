import type { ShellAuthTerminalParams, ShellAuthTerminalResponse } from "@openaide/app-server-client";

const sessions = new Map<string, AuthTerminalSession>();
const listeners = new Set<() => void>();
const notify = () => { for (const listener of listeners) listener(); };
export const subscribeAuthTerminals = (listener: () => void) => {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
};
export const authTerminalForAgent = (agentId: string) => sessions.get(agentId);

/** Ephemeral terminal presentation only. App Server owns launch, ordering and login outcome.
 * One reverse request acknowledges each frame; input is never retried or persisted. */
export class AuthTerminalSession {
  private input = "";
  private backlog: Uint8Array[] = [];
  private backlogBytes = 0;
  private output?: (bytes: Uint8Array) => void;
  private finish?: (cancel: boolean) => void;
  private idleTimer?: ReturnType<typeof setTimeout>;
  cols = 80;
  rows = 24;
  closed = false;

  constructor(readonly agentId: string, readonly id: string) {}

  attach(output: (bytes: Uint8Array) => void) {
    this.output = output;
    for (const bytes of this.backlog) output(bytes);
    return () => { this.output = undefined; };
  }

  send(input: string) {
    if (this.closed) return;
    // The same limit is enforced by App Server; pasted input cannot grow an unbounded queue.
    if (new TextEncoder().encode(this.input + input).length > 16384) { this.close(); return; }
    this.input += input;
  }

  close() {
    this.closed = true;
    this.finish?.(true);
    clearTimeout(this.idleTimer);
    this.input = "";
    this.backlog = [];
    this.backlogBytes = 0;
    if (sessions.get(this.agentId) === this) sessions.delete(this.agentId);
    notify();
  }

  exchange(params: ShellAuthTerminalParams, signal: AbortSignal): Promise<ShellAuthTerminalResponse> {
    clearTimeout(this.idleTimer);
    const bytes = Uint8Array.from(atob(params.output), (char) => char.charCodeAt(0));
    if (bytes.length) {
      // Retain bounded screen replay only for this live flow so navigating away and back
      // doesn't lose the login prompt. Completion/disconnect destroys it with the session.
      this.backlog.push(bytes);
      this.backlogBytes += bytes.length;
      while (this.backlogBytes > 262144) this.backlogBytes -= this.backlog.shift()!.length;
      this.output?.(bytes);
    }
    if (params.exited) {
      this.close();
      return Promise.resolve({ input: "", cols: this.cols, rows: this.rows, cancel: false });
    }
    return new Promise((resolve) => {
      const abort = () => this.close();
      const timer = setTimeout(() => finish(false), 100);
      const finish = (cancel: boolean) => {
        clearTimeout(timer);
        signal.removeEventListener("abort", abort);
        this.finish = undefined;
        const input = btoa(String.fromCharCode(...new TextEncoder().encode(this.input)));
        this.input = "";
        resolve({ input, cols: this.cols, rows: this.rows, cancel });
        // A vanished transport must not leave credential-bearing screen contents resident.
        if (!this.closed) this.idleTimer = setTimeout(() => this.close(), 30000);
      };
      this.finish = finish;
      signal.addEventListener("abort", abort, { once: true });
      if (signal.aborted || this.closed) this.close();
    });
  }
}

export function handleAuthTerminal(params: ShellAuthTerminalParams, signal: AbortSignal) {
  let session = sessions.get(params.agentId);
  if (!session || session.id !== params.terminalId) {
    session?.close();
    session = new AuthTerminalSession(params.agentId, params.terminalId);
    sessions.set(params.agentId, session);
    notify();
  }
  return session.exchange(params, signal);
}

export function closeAuthTerminals() {
  for (const session of sessions.values()) session.close();
}
