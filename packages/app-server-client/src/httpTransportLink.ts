import { createDiagnosticsLogger, type DiagnosticsLogger } from "./diagnostics.js";
import type { RpcMessage } from "./rpcPeer.js";
import {
  rpcMethod,
  TransportLinkError,
  type OpenedTransportLink,
  type SequencedFrame,
  type TransportLinkCloseKind,
  type TransportLinkEvents,
  type TransportLinkOpener,
  type TransportLinkResume,
} from "./transportLink.js";

export type ReliableHttpFetch = (
  input: string,
  init: {
    method: "GET" | "POST";
    headers: Record<string, string>;
    body?: string;
    signal?: AbortSignal;
    cache?: "no-store";
  },
) => Promise<{
  ok: boolean;
  status: number;
  text(): Promise<string>;
}>;

export type HttpTransportLinkOptions = {
  endpointUrl: string;
  connectionId: string;
  authToken?: string;
  fetch?: ReliableHttpFetch;
  retryDelayMs?: number;
  receiveTimeoutMs?: number;
  /** Replaces a transport that cannot complete repeated finite receive attempts. */
  maxConsecutiveReceiveTimeouts?: number;
  /** Waits for the first acknowledged client frame before polling server messages. */
  deferReceiveUntilFirstUpload?: boolean;
  /** Restarts only the replayable receive poll when a suspended runtime wakes. */
  subscribeToWake?: (wake: () => void) => () => void;
  logger?: DiagnosticsLogger;
};

type SessionHandshake = {
  transportVersion: 1;
  sessionId: string;
  serverId: string;
};

type ServerBatch = {
  frames: Array<{ sequence: number; message: RpcMessage }>;
};

const RELIABLE_UPLOAD_CHUNK_BYTES = 512 * 1024;

/**
 * Carries a reliable session over two finite HTTP directions. Upload retries
 * preserve the exact sequence and body; receive retries use the last fully
 * applied server sequence. Both directions retry inside the link, so it never
 * reports itself as interrupted: it ends only on a definite server outcome.
 */
export function createHttpLinkOpener(options: HttpTransportLinkOptions): TransportLinkOpener {
  const fetchImpl = options.fetch ?? globalThis.fetch?.bind(globalThis);
  if (!fetchImpl) throw new Error("Reliable HTTP RPC requires fetch");
  const logger = options.logger ?? createDiagnosticsLogger("openaide-reliable-http");
  return (request) => openHttpLink(options, fetchImpl, logger, request);
}

async function openHttpLink(
  options: HttpTransportLinkOptions,
  fetchImpl: ReliableHttpFetch,
  logger: DiagnosticsLogger,
  request: { events: TransportLinkEvents; resume?: TransportLinkResume; signal: AbortSignal },
): Promise<OpenedTransportLink> {
  const { events } = request;
  const connectionContext = { connection_id: options.connectionId };
  const uploads: Array<{ sequence: number; message: RpcMessage; body: string }> = [];
  const abort = new AbortController();
  const retryDelayMs = options.retryDelayMs ?? 250;
  const receiveTimeoutMs = options.receiveTimeoutMs ?? 35_000;
  const maxConsecutiveReceiveTimeouts = options.maxConsecutiveReceiveTimeouts ?? 2;
  let receiveAbort: AbortController | undefined;
  /** Highest server sequence the session reported as fully applied. */
  let lastServerSequence = request.resume?.receivedThrough ?? 0;
  let pumping = false;
  let receiving = false;
  let closed = false;
  let consecutiveReceiveTimeouts = 0;
  const pendingPosts = new Set<() => void>();
  logger.info("reliable_http_channel_created", connectionContext);
  const unsubscribeWake = options.subscribeToWake?.(() => {
    for (const interrupt of pendingPosts) interrupt();
    if (!receiveAbort || receiveAbort.signal.aborted) return;
    logger.info("reliable_http_receive_woken", connectionContext);
    receiveAbort.abort();
  });
  const stopOnSignal = () => shutdown();
  request.signal.addEventListener("abort", stopOnSignal, { once: true });

  let opened: { sessionId: string; serverId: string };
  try {
    // The session outlives its polls, so resuming needs no request of its own.
    opened = request.resume ?? await openSession();
  } catch (error) {
    shutdown();
    throw error;
  }
  if (!options.deferReceiveUntilFirstUpload || request.resume) startReceiving();

  return {
    sessionId: opened.sessionId,
    serverId: opened.serverId,
    // Uploads carry no cumulative acknowledgement; the server drops duplicates.
    peerReceivedThrough: 0,
    link: {
      send(frame) {
        if (closed) return;
        uploads.push({
          sequence: frame.sequence,
          message: frame.message,
          body: JSON.stringify({
            transport: "send",
            sessionId: opened.sessionId,
            sequence: frame.sequence,
            message: frame.message,
          }),
        });
        logger.info("reliable_http_upload_queued", {
          ...connectionContext,
          sequence: frame.sequence,
          queue_depth: uploads.length,
          method: rpcMethod(frame.message),
        });
        void pumpUploads();
      },
      acknowledge(through) {
        lastServerSequence = Math.max(lastServerSequence, through);
      },
      close() {
        if (closed) return;
        logger.info("reliable_http_channel_closed", {
          ...connectionContext,
          queued_uploads: uploads.length,
          last_server_sequence: lastServerSequence,
        });
        shutdown();
      },
    },
  };

  async function openSession(): Promise<SessionHandshake> {
    const startedAt = Date.now();
    logger.info("reliable_http_session_open_started", connectionContext);
    try {
      const response = await postWithDeadline({
        method: "POST",
        headers: baseHeaders(),
        body: JSON.stringify({ transport: "open" }),
        signal: abort.signal,
      });
      const text = await response.text();
      if (!response.ok) throw new ReliableHttpError("open", response.status, text);
      const handshake = JSON.parse(text) as SessionHandshake;
      if (handshake.transportVersion !== 1 || !handshake.sessionId || !handshake.serverId) {
        throw new Error("App Server returned an invalid reliable-session handshake");
      }
      logger.info("reliable_http_session_open_completed", {
        ...connectionContext,
        server_id: handshake.serverId,
        duration_ms: Date.now() - startedAt,
      });
      return handshake;
    } catch (error) {
      logger.warn("reliable_http_session_open_failed", {
        ...connectionContext,
        duration_ms: Date.now() - startedAt,
        ...diagnosticErrorFields(error),
      });
      throw error;
    }
  }

  async function pumpUploads() {
    if (pumping || closed) return;
    pumping = true;
    try {
      let uploadSequence: number | undefined;
      let uploadAttempt = 0;
      let uploadStartedAt = 0;
      let chunkFallback = false;
      while (!closed && uploads.length > 0) {
        const upload = uploads[0];
        if (!upload) break;
        if (uploadSequence !== upload.sequence) {
          uploadSequence = upload.sequence;
          uploadAttempt = 0;
          uploadStartedAt = Date.now();
          chunkFallback = false;
        }
        const attempt = ++uploadAttempt;
        try {
          logger.info("reliable_http_upload_started", {
            ...connectionContext,
            sequence: upload.sequence,
            attempt,
            queue_depth: uploads.length,
            method: rpcMethod(upload.message),
          });
          const response = await postWithDeadline({
            method: "POST",
            headers: baseHeaders(),
            body: upload.body,
            signal: abort.signal,
          });
          const text = await response.text();
          if (!response.ok) {
            if (!isRequestSizeRejection(response.status, text)) {
              throw new ReliableHttpError("upload", response.status, text);
            }
            chunkFallback = true;
            await uploadInChunks(upload);
          }
          uploads.shift();
          events.acknowledged(upload.sequence);
          startReceiving();
          logger.info("reliable_http_upload_completed", {
            ...connectionContext,
            sequence: upload.sequence,
            attempt,
            chunk_fallback: chunkFallback,
            duration_ms: Date.now() - uploadStartedAt,
          });
        } catch (error) {
          if (closed || isAbort(error)) return;
          if (isTerminalHttpError(error)) {
            logger.error("reliable_http_upload_failed_terminal", {
              ...connectionContext,
              sequence: upload.sequence,
              attempt,
              duration_ms: Date.now() - uploadStartedAt,
              ...diagnosticErrorFields(error),
            });
            fail(error);
            return;
          }
          logger.warn("reliable_http_upload_retry_scheduled", {
            ...connectionContext,
            sequence: upload.sequence,
            attempt,
            retry_delay_ms: retryDelayMs,
            duration_ms: Date.now() - uploadStartedAt,
            ...diagnosticErrorFields(error),
          });
          await retryDelay();
        }
      }
    } finally {
      pumping = false;
      if (!closed && uploads.length > 0) void pumpUploads();
    }
  }

  /** Retries the exact reliable-session frame without storing attachment bytes on disk. */
  async function uploadInChunks(upload: { sequence: number; body: string }) {
    const bytes = new TextEncoder().encode(upload.body);
    for (let offset = 0; offset < bytes.byteLength; offset += RELIABLE_UPLOAD_CHUNK_BYTES) {
      const data = bytes.subarray(offset, Math.min(offset + RELIABLE_UPLOAD_CHUNK_BYTES, bytes.byteLength));
      const response = await postWithDeadline({
        method: "POST",
        headers: baseHeaders(),
        body: JSON.stringify({
          transport: "chunk",
          sessionId: opened.sessionId,
          sequence: upload.sequence,
          offset,
          totalSize: bytes.byteLength,
          data: bytesToBase64(data),
        }),
        signal: abort.signal,
      });
      const text = await response.text();
      const complete = offset + data.byteLength === bytes.byteLength;
      const expectedStatus = complete ? 204 : 202;
      if (response.status !== expectedStatus) {
        throw new ReliableHttpError("chunk upload", response.status, text);
      }
    }
  }

  function startReceiving() {
    if (receiving || closed) return;
    receiving = true;
    logger.info("reliable_http_receive_started", connectionContext);
    // The first acknowledged client frame initializes product routing. Polling
    // earlier races that frame and makes the real App Server reject the session.
    void receiveLoop();
  }

  async function receiveLoop() {
    while (!closed) {
      const pollAbort = new AbortController();
      receiveAbort = pollAbort;
      const pollStartedAt = Date.now();
      let deadlineExpired = false;
      const receiveTimeout = setTimeout(() => {
        if (closed || receiveAbort !== pollAbort || pollAbort.signal.aborted) return;
        deadlineExpired = true;
        logger.warn("reliable_http_receive_timeout", {
          ...connectionContext,
          after_sequence: lastServerSequence,
          timeout_ms: receiveTimeoutMs,
        });
        pollAbort.abort();
      }, receiveTimeoutMs);
      try {
        // AbortSignal is advisory: some embedded HTTP implementations can leave
        // fetch or body reads pending after abort. Race the complete poll so a
        // replayable receive attempt always relinquishes ownership on wake or deadline.
        const interrupted = new Promise<never>((_resolve, reject) => {
          pollAbort.signal.addEventListener("abort", () => reject(receiveAbortError()), {
            once: true,
          });
        });
        const poll = async () => {
          const response = await fetchImpl(options.endpointUrl, {
            method: "GET",
            headers: {
              ...baseHeaders(),
              "X-OpenAIDE-Session-Id": opened.sessionId,
              "X-OpenAIDE-After": String(lastServerSequence),
            },
            signal: pollAbort.signal,
            // A browser serializes identical cacheable GETs behind one cache
            // entry, which would park this poll behind another window's.
            cache: "no-store",
          });
          return { response, text: await response.text() };
        };
        const { response, text } = await Promise.race([poll(), interrupted]);
        consecutiveReceiveTimeouts = 0;
        if (response.status === 204) {
          // Real polls are held by the server. Yield here as well so a test
          // double or intermediary returning immediately cannot spin the UI.
          await retryDelay();
          continue;
        }
        if (!response.ok) throw new ReliableHttpError("receive", response.status, text);
        const batch = JSON.parse(text) as ServerBatch;
        let receivedFrames = 0;
        for (const frame of batch.frames) {
          if (closed) return;
          if (frame.sequence <= lastServerSequence) continue;
          // The session applies the frame and advances the cursor through
          // `acknowledge`, so a frame it could not apply is polled again.
          events.frame(frame);
          receivedFrames += 1;
        }
        if (receivedFrames > 0) {
          logger.info("reliable_http_receive_batch_received", {
            ...connectionContext,
            frame_count: receivedFrames,
            last_server_sequence: lastServerSequence,
            duration_ms: Date.now() - pollStartedAt,
          });
        }
      } catch (error) {
        if (closed) return;
        if (isAbort(error)) {
          if (!deadlineExpired) continue;
          consecutiveReceiveTimeouts += 1;
          if (consecutiveReceiveTimeouts < maxConsecutiveReceiveTimeouts) continue;
          const stalled = new ReliableHttpReceiveStalledError(consecutiveReceiveTimeouts);
          logger.error("reliable_http_receive_stalled", {
            ...connectionContext,
            after_sequence: lastServerSequence,
            consecutive_timeout_count: consecutiveReceiveTimeouts,
          });
          fail(stalled);
          return;
        }
        if (isTerminalHttpError(error)) {
          logger.error("reliable_http_receive_failed_terminal", {
            ...connectionContext,
            after_sequence: lastServerSequence,
            duration_ms: Date.now() - pollStartedAt,
            ...diagnosticErrorFields(error),
          });
          fail(error);
          return;
        }
        logger.warn("reliable_http_receive_retry_scheduled", {
          ...connectionContext,
          after_sequence: lastServerSequence,
          retry_delay_ms: retryDelayMs,
          duration_ms: Date.now() - pollStartedAt,
          ...diagnosticErrorFields(error),
        });
        await retryDelay();
      } finally {
        clearTimeout(receiveTimeout);
        if (receiveAbort === pollAbort) receiveAbort = undefined;
      }
    }
  }

  function baseHeaders() {
    return {
      ...(options.authToken ? { Authorization: `Bearer ${options.authToken}` } : {}),
      "Content-Type": "application/json",
      "X-OpenAIDE-Connection-Id": options.connectionId,
    };
  }

  async function postWithDeadline(init: Parameters<ReliableHttpFetch>[1]) {
    const controller = new AbortController();
    let rejectPending: (error: Error) => void = () => {};
    const interrupted = new Promise<never>((_resolve, reject) => { rejectPending = reject; });
    const interrupt = () => {
      controller.abort();
      rejectPending(new Error("Reliable HTTP POST interrupted; retry the same frame"));
    };
    const close = () => { controller.abort(); rejectPending(receiveAbortError()); };
    const timer = setTimeout(interrupt, receiveTimeoutMs);
    const isUpload = init.body !== '{"transport":"open"}';
    if (isUpload) pendingPosts.add(interrupt);
    abort.signal.addEventListener("abort", close, { once: true });
    try {
      if (abort.signal.aborted) close();
      const read = async () => {
        const result = await fetchImpl(options.endpointUrl, { ...init, signal: controller.signal });
        const body = await result.text();
        return { ok: result.ok, status: result.status, text: async () => body };
      };
      return await Promise.race([read(), interrupted]);
    } finally {
      clearTimeout(timer);
      pendingPosts.delete(interrupt);
      abort.signal.removeEventListener("abort", close);
    }
  }

  function retryDelay() {
    if (retryDelayMs === 0) return Promise.resolve();
    return new Promise<void>((resolve) => setTimeout(resolve, retryDelayMs));
  }

  /** Releases every request and timer; safe to repeat. */
  function shutdown() {
    closed = true;
    request.signal.removeEventListener("abort", stopOnSignal);
    unsubscribeWake?.();
    abort.abort();
    receiveAbort?.abort();
  }

  function fail(error: unknown) {
    if (closed) return;
    logger.error("reliable_http_channel_failed", {
      ...connectionContext,
      ...diagnosticErrorFields(error),
    });
    shutdown();
    events.closed({ kind: error instanceof TransportLinkError ? error.kind : "rejected", error });
  }
}

function diagnosticErrorFields(error: unknown) {
  return {
    error_kind: error instanceof Error && error.name ? error.name : typeof error,
    ...reliableHttpErrorDiagnosticFields(error),
  };
}

function isRequestSizeRejection(status: number, body: string) {
  return status === 413 || (status === 403 && /<\s*(?:!doctype|html)\b/i.test(body));
}

function bytesToBase64(bytes: Uint8Array) {
  let binary = "";
  const binaryChunkBytes = 0x8000;
  for (let offset = 0; offset < bytes.byteLength; offset += binaryChunkBytes) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + binaryChunkBytes));
  }
  return btoa(binary);
}

/** An HTTP answer from the reliable-session endpoint that was not a success. */
class ReliableHttpError extends TransportLinkError {
  constructor(
    readonly operation: string,
    readonly status: number,
    readonly body: string,
  ) {
    super(
      httpCloseKind(status, body),
      `App Server reliable-session ${operation} failed with HTTP ${status}: ${body}`,
    );
  }

  override diagnosticFields(): Record<string, unknown> {
    let responseCode: string | undefined;
    try {
      const code = (JSON.parse(this.body) as { code?: unknown }).code;
      if (typeof code === "string" && SAFE_RELIABLE_HTTP_RESPONSE_CODES.has(code)) {
        responseCode = code;
      }
    } catch {
      // Empty and non-JSON intermediary bodies remain classified by operation and status.
    }
    return {
      error_kind: "reliable_http",
      transport_operation_kind: this.operation,
      http_status: this.status,
      ...(responseCode ? { response_code: responseCode } : {}),
    };
  }
}

class ReliableHttpReceiveStalledError extends TransportLinkError {
  override readonly name = "ReliableHttpReceiveStalledError";

  constructor(readonly consecutiveTimeoutCount: number) {
    super("receiveStalled", "App Server reliable receive remained stalled after repeated deadlines");
  }
}

/** Statuses outside this mapping are transient and retried inside the link. */
function httpCloseKind(status: number, body: string): TransportLinkCloseKind {
  // A gone session is safe to replace, but the interrupted RPC is still ambiguous.
  if (status === 410) return "sessionExpired";
  if (status === 409) {
    // A bounded receive replay gap requires fresh product-state subscription baselines.
    try {
      if ((JSON.parse(body) as { resyncRequired?: unknown }).resyncRequired === true) {
        return "replayExpired";
      }
    } catch {
      // A conflict without the marker is an ordinary rejection.
    }
    return "rejected";
  }
  return [400, 401, 403].includes(status) ? "rejected" : "interrupted";
}

const SAFE_RELIABLE_HTTP_RESPONSE_CODES = new Set([
  "invalid_connection_id",
  "invalid_after",
  "invalid_chunk",
  "invalid_chunk_base64",
  "invalid_chunk_envelope",
  "invalid_chunk_utf8",
  "invalid_jsonrpc_version",
  "invalid_request_envelope",
  "invalid_request_id",
  "invalid_upload_envelope",
  "malformed_json",
  "missing_method",
  "missing_after",
  "missing_session_id",
  "nested_protocol_rejected",
  "unsupported_notification",
]);

/**
 * Extracts Support Export-safe transport facts without retaining a response
 * body, endpoint, connection identity, session identity, or credential.
 */
export function reliableHttpErrorDiagnosticFields(error: unknown): Record<string, unknown> {
  return error instanceof ReliableHttpError ? error.diagnosticFields() : {};
}

function isTerminalHttpError(error: unknown) {
  return error instanceof ReliableHttpError && error.kind !== "interrupted";
}

function isAbort(error: unknown) {
  return error instanceof Error && error.name === "AbortError";
}

function receiveAbortError() {
  const error = new Error("Reliable HTTP receive was interrupted");
  error.name = "AbortError";
  return error;
}
