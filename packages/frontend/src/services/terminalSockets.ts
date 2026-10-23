import type { TerminalAttach } from "@openaide/app-server-client";

/** The part of a browser WebSocket this stream uses; tests supply one. */
export type TerminalWebSocket = {
  binaryType: string;
  readyState: number;
  onopen: (() => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  onclose: ((event: { code: number }) => void) | null;
  onerror: (() => void) | null;
  send(data: string | ArrayBufferLike | Uint8Array): void;
  close(code?: number): void;
};

/**
 * `connecting` until the first replay, `live` while bytes flow, `reconnecting` after a lost
 * socket, `ended` once the shell or Terminal is gone, `failed` on a fault a retry cannot fix.
 */
export type TerminalStreamState = "connecting" | "live" | "reconnecting" | "ended" | "failed";

export type TerminalStreamLog = (level: "info" | "warn", event: string, fields: Record<string, string | number>) => void;

export type TerminalStreamOptions = {
  terminalId: string;
  /** The token `terminals/open` returned, spent by the first socket. */
  initialAttach?: TerminalAttach;
  /** Asks `terminals/attach` for a fresh token; rejects when the Terminal is gone. */
  attach(): Promise<TerminalAttach>;
  socketUrl(): string;
  size(): { cols: number; rows: number };
  /** Each socket starts with the replay, so a reconnect clears the screen first. */
  onReset(): void;
  onOutput(bytes: Uint8Array): void;
  onState(state: TerminalStreamState): void;
  log?: TerminalStreamLog;
  createSocket?: (url: string) => TerminalWebSocket;
  /** Delay before each reconnect, by attempt; the last repeats. */
  backoffMs?: readonly number[];
  /** How long a socket may take to answer `ready` before it counts as lost. */
  readyTimeoutMs?: number;
  /** How long a live socket may go without a message, pings included, before it counts as lost. */
  silenceTimeoutMs?: number;
};

export type TerminalStream = {
  write(data: string | Uint8Array): void;
  resize(cols: number, rows: number): void;
  /** Leaves the shell running for every other viewer and for a later return. */
  detach(): void;
};

const OPEN = 1;
const CLOSE_ENDED = 1000;
const CLOSE_VIOLATION = 4400;
const DEFAULT_BACKOFF_MS = [250, 1_000, 2_000, 4_000, 8_000] as const;
// A half-open path neither answers nor closes; App Server gives the hello the same budget.
const DEFAULT_READY_TIMEOUT_MS = 10_000;
// App Server pings every 15 s; three missed pings mean a suspended phone or a dropped network
// that the browser would otherwise notice only at its own TCP timeout.
const DEFAULT_SILENCE_TIMEOUT_MS = 45_000;
const PONG = JSON.stringify({ type: "pong" });
const encoder = new TextEncoder();

/**
 * One viewer of one Terminal (ADR-0071). The App Server owns the shell and its replay; this
 * only carries bytes and reattaches after a lost socket, because every socket's replay
 * restores the screen. Logs carry the Terminal id, attempt, close code and duration only.
 */
export function connectTerminalStream(options: TerminalStreamOptions): TerminalStream {
  const log = options.log ?? consoleLog;
  const createSocket = options.createSocket ?? ((url) => new WebSocket(url) as unknown as TerminalWebSocket);
  const backoff = options.backoffMs ?? DEFAULT_BACKOFF_MS;
  let pendingAttach = options.initialAttach;
  let socket: TerminalWebSocket | undefined;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let readyTimer: ReturnType<typeof setTimeout> | undefined;
  let silenceTimer: ReturnType<typeof setTimeout> | undefined;
  let attempt = 0;
  let detached = false;
  let state: TerminalStreamState | undefined;

  const setState = (next: TerminalStreamState) => {
    if (state === next) return;
    state = next;
    options.onState(next);
  };
  const fields = (extra: Record<string, string | number> = {}) => ({
    terminal_id: options.terminalId,
    attempt,
    ...extra,
  });

  async function open() {
    attempt += 1;
    const started = performance.now();
    log("info", "terminal_stream_open_started", fields());
    let attach: TerminalAttach;
    try {
      attach = pendingAttach ?? (await options.attach());
    } catch {
      if (detached) return;
      // Usually the App Server session is away, which a later attempt outlasts. A Terminal that
      // no longer exists leaves the Terminals list, and its page detaches this stream.
      log("warn", "terminal_stream_attach_failed", fields({ duration_ms: elapsed(started) }));
      scheduleRetry();
      return;
    }
    pendingAttach = undefined;
    if (detached) return;
    let next: TerminalWebSocket;
    try {
      next = createSocket(options.socketUrl());
    } catch {
      log("warn", "terminal_stream_open_failed", fields({ reason_code: "socket_unavailable" }));
      setState("failed");
      return;
    }
    socket = next;
    next.binaryType = "arraybuffer";
    let ready = false;
    readyTimer = setTimeout(() => {
      readyTimer = undefined;
      if (socket !== next || ready) return;
      socket = undefined;
      next.close();
      log("warn", "terminal_stream_open_failed", fields({ reason_code: "ready_timeout", duration_ms: elapsed(started) }));
      scheduleRetry();
    }, options.readyTimeoutMs ?? DEFAULT_READY_TIMEOUT_MS);
    next.onopen = () => {
      const { cols, rows } = options.size();
      next.send(JSON.stringify({ type: "hello", token: attach.token, cols, rows }));
    };
    const lose = (reason: string) => {
      if (socket !== next) return;
      socket = undefined;
      clearTimeout(silenceTimer);
      next.close();
      log("warn", "terminal_stream_lost", fields({ reason_code: reason, ready: 1, duration_ms: elapsed(started) }));
      scheduleRetry();
    };
    const heard = () => {
      clearTimeout(silenceTimer);
      silenceTimer = setTimeout(() => lose("silent"), options.silenceTimeoutMs ?? DEFAULT_SILENCE_TIMEOUT_MS);
    };
    next.onmessage = (event) => {
      if (ready) heard();
      if (typeof event.data === "string") {
        if (ready) {
          if (isPing(event.data)) next.send(PONG);
          return;
        }
        ready = true;
        heard();
        clearTimeout(readyTimer);
        log("info", "terminal_stream_open_completed", fields({ duration_ms: elapsed(started) }));
        attempt = 0;
        options.onReset();
        setState("live");
        return;
      }
      if (event.data instanceof ArrayBuffer) options.onOutput(new Uint8Array(event.data));
    };
    next.onerror = () => undefined;
    next.onclose = ({ code }) => {
      if (socket !== next) return;
      socket = undefined;
      clearTimeout(readyTimer);
      clearTimeout(silenceTimer);
      const closed = fields({ close_code: code, ready: ready ? 1 : 0, duration_ms: elapsed(started) });
      if (detached) return;
      if (code === CLOSE_ENDED) {
        log("info", "terminal_stream_ended", closed);
        setState("ended");
      } else if (code === CLOSE_VIOLATION) {
        log("warn", "terminal_stream_failed", closed);
        setState("failed");
      } else {
        // Lost, token refused, or dropped for falling behind: a fresh attach replays.
        log("warn", "terminal_stream_lost", closed);
        scheduleRetry();
      }
    };
  }

  function scheduleRetry() {
    setState("reconnecting");
    const delay = backoff[Math.min(attempt - 1, backoff.length - 1)] ?? 0;
    log("info", "terminal_stream_retry_scheduled", fields({ delay_ms: delay }));
    retryTimer = setTimeout(() => {
      retryTimer = undefined;
      void open();
    }, delay);
  }

  setState("connecting");
  void open();

  return {
    write(data) {
      if (socket?.readyState !== OPEN || state !== "live") return;
      socket.send(typeof data === "string" ? encoder.encode(data) : data);
    },
    resize(cols, rows) {
      if (socket?.readyState !== OPEN || state !== "live") return;
      socket.send(JSON.stringify({ type: "resize", cols, rows }));
    },
    detach() {
      if (detached) return;
      detached = true;
      if (retryTimer) clearTimeout(retryTimer);
      clearTimeout(readyTimer);
      clearTimeout(silenceTimer);
      socket?.close(1000);
      socket = undefined;
      log("info", "terminal_stream_detached", fields());
    },
  };
}

function isPing(text: string) {
  try {
    return (JSON.parse(text) as { type?: unknown }).type === "ping";
  } catch {
    return false;
  }
}

function elapsed(started: number) {
  return Math.round(performance.now() - started);
}

function consoleLog(level: "info" | "warn", event: string, fields: Record<string, string | number>) {
  const line = [event, ...Object.entries(fields).map(([key, value]) => `${key}=${value}`)].join(" ");
  if (level === "warn") console.warn(line);
  else console.info(line);
}
