import { createDiagnosticsLogger, type DiagnosticsLogger } from "./diagnostics.js";
import { CLIENT_HEARTBEAT } from "./generated/protocol.js";
import type { RpcMessage } from "./rpcPeer.js";
import {
  rpcMethod,
  TransportLinkError,
  type OpenedTransportLink,
  type TransportLinkCloseKind,
  type TransportLinkOpener,
} from "./transportLink.js";

/** The part of a WebSocket this link uses; lets tests and hosts supply one. */
export type TransportWebSocket = {
  send(data: string): void;
  close(code?: number, reason?: string): void;
  onopen: (() => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  onclose: ((event: { code: number }) => void) | null;
  onerror: (() => void) | null;
};

export type WebSocketTransportLinkOptions = {
  /** The HTTP endpoint of the App Server; the socket uses the same address. */
  endpointUrl: string;
  connectionId: string;
  /** Sent in the first message, never in the URL. A proxying shell omits it. */
  authToken?: string;
  createSocket?: (url: string) => TransportWebSocket;
  openTimeoutMs?: number;
  pingIntervalMs?: number;
  pongTimeoutMs?: number;
  /** Deadline for the probe sent when a suspended runtime wakes. */
  wakePongTimeoutMs?: number;
  /** Coalesces acknowledgements of server frames into one message. */
  ackDelayMs?: number;
  subscribeToWake?: (wake: () => void) => () => void;
  logger?: DiagnosticsLogger;
};

type ServerMessage =
  | { type: "ready"; transportVersion: number; sessionId: string; serverId: string; receivedThrough: number }
  | { type: "frame"; sequence: number; message: RpcMessage }
  | { type: "ack"; through: number }
  | { type: "ping" }
  | { type: "pong" };

const TRANSPORT_VERSION = 1;
const CLOSE_NORMAL = 1000;

/** Definite App Server decisions; any other close code is a lost socket. */
const CLOSE_KINDS: Record<number, TransportLinkCloseKind> = {
  4000: "rejected", // superseded by a newer link of the same session
  4400: "rejected", // protocol violation
  4401: "rejected", // unauthorized
  4403: "rejected", // forbidden
  4409: "replayExpired",
  4410: "sessionExpired",
};

/**
 * Carries a reliable session over one WebSocket: client frames, server frames,
 * and acknowledgements share the socket, so the server pushes a frame the
 * moment it exists. A socket that ends without a definite close code is
 * reported as interrupted, and the session resumes on a new socket.
 */
export function createWebSocketLinkOpener(
  options: WebSocketTransportLinkOptions,
): TransportLinkOpener {
  const logger = options.logger ?? createDiagnosticsLogger("openaide-reliable-websocket");
  const createSocket = options.createSocket ?? defaultSocketFactory();
  const url = webSocketUrl(options.endpointUrl, options.connectionId);
  const connectionContext = { connection_id: options.connectionId };
  const openTimeoutMs = options.openTimeoutMs ?? 10_000;
  const pingIntervalMs = options.pingIntervalMs ?? 10_000;
  const pongTimeoutMs = options.pongTimeoutMs ?? 5_000;
  const wakePongTimeoutMs = options.wakePongTimeoutMs ?? 2_000;
  const ackDelayMs = options.ackDelayMs ?? 100;

  return ({ events, resume, signal }) => new Promise<OpenedTransportLink>((resolve, reject) => {
    const startedAt = Date.now();
    let socket: TransportWebSocket;
    let ready = false;
    let ended = false;
    let framesSent = 0;
    let framesReceived = 0;
    let pendingAck: number | undefined;
    let ackTimer: ReturnType<typeof setTimeout> | undefined;
    let pingTimer: ReturnType<typeof setInterval> | undefined;
    let pongTimer: ReturnType<typeof setTimeout> | undefined;
    let unsubscribeWake: (() => void) | undefined;
    const openTimer = setTimeout(() => lose("open_timeout"), openTimeoutMs);
    const abortOpen = () => end("client_closed", CLOSE_NORMAL);
    signal.addEventListener("abort", abortOpen, { once: true });

    logger.info("reliable_websocket_open_started", { ...connectionContext, resumed: Boolean(resume) });
    try {
      socket = createSocket(url);
    } catch {
      // A constructor failure can carry the address; only its class is kept.
      end("socket_unavailable");
      return;
    }
    socket.onopen = () => {
      write({
        type: "hello",
        transportVersion: TRANSPORT_VERSION,
        connectionId: options.connectionId,
        receivedThrough: resume?.receivedThrough ?? 0,
        ...(resume ? { sessionId: resume.sessionId } : {}),
        ...(options.authToken ? { authToken: options.authToken } : {}),
      });
    };
    socket.onmessage = (event) => {
      if (ended) return;
      let message: ServerMessage;
      try {
        if (typeof event.data !== "string") throw new Error("binary message");
        message = JSON.parse(event.data) as ServerMessage;
      } catch {
        lose("invalid_message");
        return;
      }
      // Any message proves the peer is alive, not only the pong.
      clearPongDeadline();
      if (!ready) {
        if (message.type !== "ready" || message.transportVersion !== TRANSPORT_VERSION
          || !message.sessionId || !message.serverId) {
          lose("invalid_ready");
          return;
        }
        becomeReady(message);
        return;
      }
      switch (message.type) {
        case "frame":
          framesReceived += 1;
          events.frame({ sequence: message.sequence, message: message.message });
          break;
        case "ack":
          events.acknowledged(message.through);
          break;
        case "ping":
          write({ type: "pong" });
          break;
        case "pong":
          break;
        default:
          lose("invalid_message");
      }
    };
    socket.onclose = (event) => {
      const kind = CLOSE_KINDS[event.code] ?? "interrupted";
      finish(kind, kind === "interrupted" ? "socket_closed" : "server_closed", event.code);
    };
    // The close event that follows carries the outcome; nothing to read here.
    socket.onerror = () => undefined;

    function becomeReady(message: Extract<ServerMessage, { type: "ready" }>) {
      ready = true;
      clearTimeout(openTimer);
      signal.removeEventListener("abort", abortOpen);
      logger.info("reliable_websocket_open_completed", {
        ...connectionContext,
        resumed: Boolean(resume),
        server_id: message.serverId,
        duration_ms: Date.now() - startedAt,
      });
      pingTimer = setInterval(() => probe(pongTimeoutMs), pingIntervalMs);
      unsubscribeWake = options.subscribeToWake?.(() => {
        // A socket can outlive a suspended runtime only in appearance; a
        // short probe replaces it before the next request waits on it.
        logger.info("reliable_websocket_wake_probe", connectionContext);
        clearPongDeadline();
        probe(wakePongTimeoutMs);
      });
      resolve({
        sessionId: message.sessionId,
        serverId: message.serverId,
        peerReceivedThrough: message.receivedThrough,
        link: {
          send(frame) {
            if (ended) return;
            framesSent += 1;
            const method = rpcMethod(frame.message);
            // Heartbeats are the healthy steady state; the close event counts them.
            if (method !== CLIENT_HEARTBEAT) {
              logger.info("reliable_websocket_frame_sent", {
                ...connectionContext,
                sequence: frame.sequence,
                method,
              });
            }
            write({ type: "frame", sequence: frame.sequence, message: frame.message });
          },
          acknowledge(through) {
            if (ended) return;
            pendingAck = through;
            ackTimer ??= setTimeout(flushAck, ackDelayMs);
          },
          close() {
            flushAck();
            end("client_closed", CLOSE_NORMAL);
          },
        },
      });
    }

    function probe(deadlineMs: number) {
      if (ended || pongTimer) return;
      write({ type: "ping" });
      pongTimer = setTimeout(() => lose("pong_timeout"), deadlineMs);
    }

    function clearPongDeadline() {
      if (pongTimer) clearTimeout(pongTimer);
      pongTimer = undefined;
    }

    function flushAck() {
      if (ackTimer) clearTimeout(ackTimer);
      ackTimer = undefined;
      if (ended || pendingAck === undefined) return;
      write({ type: "ack", through: pendingAck });
      pendingAck = undefined;
    }

    function write(message: Record<string, unknown>) {
      try {
        socket.send(JSON.stringify(message));
      } catch {
        // The socket closed underneath the write; the session resends on resume.
        lose("write_failed");
      }
    }

    /** The client gave up on a socket the server never ruled on. */
    function lose(reason: string) {
      finish("interrupted", reason);
    }

    /** Reports the end of a socket that the session did not close itself. */
    function finish(kind: TransportLinkCloseKind, reason: string, closeCode?: number) {
      if (ended) return;
      const wasReady = ready;
      end(reason, closeCode === undefined ? CLOSE_NORMAL : undefined, kind, closeCode);
      const error = new WebSocketLinkError(kind, reason, closeCode);
      if (wasReady) events.closed({ kind, error });
      else reject(error);
    }

    /** Releases the socket and its timers once, and records why. */
    function end(
      reason: string,
      localCloseCode?: number,
      kind?: TransportLinkCloseKind,
      closeCode?: number,
    ) {
      if (ended) return;
      ended = true;
      clearTimeout(openTimer);
      if (ackTimer) clearTimeout(ackTimer);
      if (pingTimer) clearInterval(pingTimer);
      clearPongDeadline();
      unsubscribeWake?.();
      signal.removeEventListener("abort", abortOpen);
      const fields = {
        ...connectionContext,
        resumed: Boolean(resume),
        reason_code: reason,
        frames_sent: framesSent,
        frames_received: framesReceived,
        duration_ms: Date.now() - startedAt,
        ...(kind ? { link_close_kind: kind } : {}),
        ...(closeCode === undefined ? {} : { close_code: closeCode }),
      };
      if (!ready) logger.warn("reliable_websocket_open_failed", fields);
      else if (kind) logger.warn("reliable_websocket_closed", fields);
      else logger.info("reliable_websocket_closed", fields);
      if (!ready && !kind) reject(new WebSocketLinkError("interrupted", reason));
      if (localCloseCode !== undefined) {
        try {
          socket?.close(localCloseCode, reason);
        } catch {
          // Already closing; the outcome was recorded above.
        }
      }
    }
  });
}

class WebSocketLinkError extends TransportLinkError {
  override readonly name = "WebSocketLinkError";

  constructor(
    kind: TransportLinkCloseKind,
    readonly reason: string,
    readonly closeCode?: number,
  ) {
    super(kind, `App Server WebSocket link ended: ${reason}`);
  }

  override diagnosticFields(): Record<string, unknown> {
    return {
      error_kind: "reliable_websocket",
      link_close_kind: this.kind,
      reason_code: this.reason,
      ...(this.closeCode === undefined ? {} : { close_code: this.closeCode }),
    };
  }
}

function defaultSocketFactory(): (url: string) => TransportWebSocket {
  const Socket = (globalThis as { WebSocket?: new (url: string) => unknown }).WebSocket;
  if (!Socket) throw new Error("Reliable WebSocket RPC requires WebSocket");
  return (url) => new Socket(url) as TransportWebSocket;
}

/**
 * Maps the HTTP endpoint onto the socket scheme; a relative one is same-origin.
 * The connection id rides along only so a proxy can correlate its logs: it
 * identifies, the hello authenticates.
 */
function webSocketUrl(endpointUrl: string, connectionId: string) {
  const base = (globalThis as { location?: { href?: string } }).location?.href;
  const url = new URL(endpointUrl, base);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  url.searchParams.set("connectionId", connectionId);
  return url.toString();
}
