import type { RpcMessage, RpcMessageChannel } from "./rpcPeer.js";
import { createDiagnosticsLogger, type DiagnosticsLogger } from "./diagnostics.js";

export type ReliableHttpFetch = (
  input: string,
  init: {
    method: "GET" | "POST";
    headers: Record<string, string>;
    body?: string;
    signal?: AbortSignal;
  },
) => Promise<{
  ok: boolean;
  status: number;
  text(): Promise<string>;
}>;

export type ReliableHttpMessageChannel = RpcMessageChannel & {
  /** Resolves once the transport session has identified its App Server instance. */
  ready(): Promise<{ serverId: string }>;
  close(): void;
};

export type ReliableHttpMessageChannelOptions = {
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
 * Hides the two finite HTTP directions behind one logical message channel.
 * Upload retries preserve the exact sequence and body; receive retries use the
 * last fully delivered server sequence so neither path can silently skip data.
 */
export function createReliableHttpMessageChannel(
  options: ReliableHttpMessageChannelOptions,
): ReliableHttpMessageChannel {
  const fetchImpl = options.fetch ?? globalThis.fetch?.bind(globalThis);
  if (!fetchImpl) throw new Error("Reliable HTTP RPC requires fetch");
  const logger = options.logger ?? createDiagnosticsLogger("openaide-reliable-http");
  const connectionContext = { connection_id: options.connectionId };
  const listeners = new Set<(message: RpcMessage) => void>();
  const errorListeners = new Set<(error: unknown) => void>();
  const uploads: Array<{ sequence: number; message: RpcMessage; body: string }> = [];
  const abort = new AbortController();
  const retryDelayMs = options.retryDelayMs ?? 250;
  const receiveTimeoutMs = options.receiveTimeoutMs ?? 35_000;
  const maxConsecutiveReceiveTimeouts = options.maxConsecutiveReceiveTimeouts ?? 2;
  let receiveAbort: AbortController | undefined;
  let nextClientSequence = 1;
  let lastServerSequence = 0;
  let pumping = false;
  let receiving = false;
  let closed = false;
  let terminalError: unknown;
  let consecutiveReceiveTimeouts = 0;
  logger.info("reliable_http_channel_created", connectionContext);
  const unsubscribeWake = options.subscribeToWake?.(() => {
    if (!receiveAbort || receiveAbort.signal.aborted) return;
    logger.info("reliable_http_receive_woken", connectionContext);
    receiveAbort.abort();
  });
  const session = openSession().catch((error) => {
    fail(error);
    throw error;
  });
  if (!options.deferReceiveUntilFirstUpload) startReceiving();

  return {
    ready: async () => {
      const opened = await session;
      return { serverId: opened.serverId };
    },
    send(message) {
      if (closed) throw new Error("Reliable HTTP RPC channel is closed");
      if (terminalError) throw terminalError;
      const sequence = nextClientSequence++;
      uploads.push({
        sequence,
        message,
        body: JSON.stringify({
          transport: "send",
          sessionId: "__SESSION_ID__",
          sequence,
          message,
        }),
      });
      logger.info("reliable_http_upload_queued", {
        ...connectionContext,
        sequence,
        queue_depth: uploads.length,
        method: rpcMethod(message),
      });
      void pumpUploads();
    },
    subscribe(receive) {
      listeners.add(receive);
      return () => listeners.delete(receive);
    },
    subscribeErrors(receive) {
      errorListeners.add(receive);
      if (terminalError) receive(terminalError);
      return () => errorListeners.delete(receive);
    },
    close() {
      if (closed) return;
      closed = true;
      logger.info("reliable_http_channel_closed", {
        ...connectionContext,
        queued_uploads: uploads.length,
        last_server_sequence: lastServerSequence,
      });
      unsubscribeWake?.();
      abort.abort();
      receiveAbort?.abort();
      listeners.clear();
      errorListeners.clear();
    },
  };

  async function openSession(): Promise<SessionHandshake> {
    const startedAt = Date.now();
    logger.info("reliable_http_session_open_started", connectionContext);
    try {
      const response = await fetchImpl(options.endpointUrl, {
        method: "POST",
        headers: baseHeaders(),
        body: JSON.stringify({ transport: "open" }),
        signal: abort.signal,
      });
      const text = await response.text();
      if (!response.ok) throw httpError("open", response.status, text);
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
      const opened = await session;
      let uploadSequence: number | undefined;
      let uploadAttempt = 0;
      let uploadStartedAt = 0;
      let chunkFallback = false;
      while (!closed && uploads.length > 0) {
        const upload = uploads[0];
        if (!upload) break;
        const body = upload.body.replace("__SESSION_ID__", opened.sessionId);
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
          const response = await fetchImpl(options.endpointUrl, {
            method: "POST",
            headers: baseHeaders(),
            body,
            signal: abort.signal,
          });
          const text = await response.text();
          if (!response.ok) {
            if (isRequestSizeRejection(response.status, text)) {
              chunkFallback = true;
              await uploadInChunks(opened, upload, body);
              uploads.shift();
              startReceiving();
              logger.info("reliable_http_upload_completed", {
                ...connectionContext,
                sequence: upload.sequence,
                attempt,
                chunk_fallback: true,
                duration_ms: Date.now() - uploadStartedAt,
              });
              continue;
            }
            throw httpError("upload", response.status, text);
          }
          uploads.shift();
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
      if (!closed && !terminalError && uploads.length > 0) void pumpUploads();
    }
  }

  /** Retries the exact reliable-session frame without storing attachment bytes on disk. */
  async function uploadInChunks(
    opened: SessionHandshake,
    upload: { sequence: number },
    body: string,
  ) {
    const bytes = new TextEncoder().encode(body);
    for (let offset = 0; offset < bytes.byteLength; offset += RELIABLE_UPLOAD_CHUNK_BYTES) {
      const data = bytes.subarray(offset, Math.min(offset + RELIABLE_UPLOAD_CHUNK_BYTES, bytes.byteLength));
      const response = await fetchImpl(options.endpointUrl, {
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
        throw httpError("chunk upload", response.status, text);
      }
    }
  }

  function startReceiving() {
    if (receiving || closed || terminalError) return;
    receiving = true;
    logger.info("reliable_http_receive_started", connectionContext);
    // The first acknowledged client frame initializes product routing. Polling
    // earlier races that frame and makes the real App Server reject the session.
    void receiveLoop();
  }

  async function receiveLoop() {
    let opened: SessionHandshake;
    try {
      opened = await session;
    } catch {
      return;
    }
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
        if (!response.ok) throw httpError("receive", response.status, text);
        const batch = JSON.parse(text) as ServerBatch;
        let receivedFrames = 0;
        for (const frame of batch.frames) {
          if (frame.sequence <= lastServerSequence) continue;
          if (frame.sequence !== lastServerSequence + 1) {
            throw new Error(`App Server session sequence gap: expected ${lastServerSequence + 1}`);
          }
          for (const listener of listeners) listener(frame.message);
          lastServerSequence = frame.sequence;
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

  function retryDelay() {
    if (retryDelayMs === 0) return Promise.resolve();
    return new Promise<void>((resolve) => setTimeout(resolve, retryDelayMs));
  }

  function fail(error: unknown) {
    if (terminalError) return;
    terminalError = error;
    logger.error("reliable_http_channel_failed", {
      ...connectionContext,
      ...diagnosticErrorFields(error),
    });
    abort.abort();
    for (const listener of errorListeners) listener(error);
  }
}

function rpcMethod(message: RpcMessage) {
  return "method" in message && typeof message.method === "string"
    ? message.method
    : "response";
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

function httpError(operation: string, status: number, body: string) {
  return new ReliableHttpError(operation, status, body);
}

class ReliableHttpError extends Error {
  constructor(
    readonly operation: string,
    readonly status: number,
    readonly body: string,
  ) {
    super(`App Server reliable-session ${operation} failed with HTTP ${status}: ${body}`);
  }
}

class ReliableHttpReceiveStalledError extends Error {
  readonly name = "ReliableHttpReceiveStalledError";

  constructor(readonly consecutiveTimeoutCount: number) {
    super("App Server reliable receive remained stalled after repeated deadlines");
  }
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
  if (!(error instanceof ReliableHttpError)) return {};
  let responseCode: string | undefined;
  try {
    const code = (JSON.parse(error.body) as { code?: unknown }).code;
    if (typeof code === "string" && SAFE_RELIABLE_HTTP_RESPONSE_CODES.has(code)) {
      responseCode = code;
    }
  } catch {
    // Empty and non-JSON intermediary bodies remain classified by operation and status.
  }
  return {
    error_kind: "reliable_http",
    transport_operation_kind: error.operation,
    http_status: error.status,
    ...(responseCode ? { response_code: responseCode } : {}),
  };
}

/** A gone session is safe to replace, but the interrupted RPC is still ambiguous. */
export function isReliableHttpSessionExpired(error: unknown) {
  return error instanceof ReliableHttpError && error.status === 410;
}

/** A bounded receive replay gap requires fresh product-state subscription baselines. */
export function isReliableHttpReplayExpired(error: unknown) {
  if (!(error instanceof ReliableHttpError) || error.status !== 409) return false;
  try {
    const payload = JSON.parse(error.body) as { resyncRequired?: unknown };
    return payload.resyncRequired === true;
  } catch {
    return false;
  }
}

/** A receive path that cannot honor finite polls must move to a fresh generation. */
export function isReliableHttpReceiveStalled(error: unknown) {
  return error instanceof ReliableHttpReceiveStalledError;
}

function isTerminalHttpError(error: unknown) {
  return error instanceof ReliableHttpError
    && [400, 401, 403, 409, 410].includes(error.status);
}

function isAbort(error: unknown) {
  return error instanceof Error && error.name === "AbortError";
}

function receiveAbortError() {
  const error = new Error("Reliable HTTP receive was interrupted");
  error.name = "AbortError";
  return error;
}
