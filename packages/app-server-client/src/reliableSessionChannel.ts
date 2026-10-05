import { createDiagnosticsLogger, type DiagnosticsLogger } from "./diagnostics.js";
import type { RpcMessage, RpcMessageChannel } from "./rpcPeer.js";
import {
  transportLinkCloseKind,
  transportLinkErrorDiagnosticFields,
  type SequencedFrame,
  type TransportLink,
  type TransportLinkEvents,
  type TransportLinkOpener,
  type TransportLinkResume,
} from "./transportLink.js";

export type ReliableSessionChannel = RpcMessageChannel & {
  /** Resolves once the transport session has identified its App Server instance. */
  ready(): Promise<{ serverId: string }>;
  close(): void;
};

export type ReliableSessionChannelOptions = {
  openLink: TransportLinkOpener;
  connectionId: string;
  /** First delay before reopening a lost link; doubles up to `maxRetryDelayMs`. */
  retryDelayMs?: number;
  maxRetryDelayMs?: number;
  logger?: DiagnosticsLogger;
};

/**
 * Owns the reliable session above a replaceable link: client sequence numbers,
 * the frames the server has not acknowledged, and the cursor into the server
 * sequence. A link that is merely lost is reopened against the same session and
 * the unacknowledged tail is resent, so neither direction can skip or repeat a
 * message. Any definite link outcome ends the channel for the layer above.
 */
export function createReliableSessionChannel(
  options: ReliableSessionChannelOptions,
): ReliableSessionChannel {
  const logger = options.logger ?? createDiagnosticsLogger("openaide-reliable-session");
  const connectionContext = { connection_id: options.connectionId };
  const retryDelayMs = options.retryDelayMs ?? 250;
  const maxRetryDelayMs = Math.max(options.maxRetryDelayMs ?? 5_000, retryDelayMs);
  const listeners = new Set<(message: RpcMessage) => void>();
  const errorListeners = new Set<(error: unknown) => void>();
  const unacknowledged: SequencedFrame[] = [];
  const abort = new AbortController();
  let nextClientSequence = 1;
  let receivedThrough = 0;
  let link: TransportLink | undefined;
  /** Identity of the open attempt whose events are still authoritative. */
  let currentAttempt: object | undefined;
  let session: { sessionId: string; serverId: string } | undefined;
  let resuming = false;
  let closed = false;
  let terminalError: unknown;

  const opened = open(undefined, 1).catch((error) => {
    // No session exists yet, so there is nothing to resume: the connection
    // layer decides whether a fresh channel is worth another attempt.
    fail(error);
    throw error;
  });
  opened.catch(() => undefined);

  return {
    ready: async () => ({ serverId: (await opened).serverId }),
    send(message) {
      if (closed) throw new Error("Reliable session channel is closed");
      if (terminalError) throw terminalError;
      const frame = { sequence: nextClientSequence++, message };
      unacknowledged.push(frame);
      link?.send(frame);
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
      logger.info("reliable_session_channel_closed", {
        ...connectionContext,
        unacknowledged_frames: unacknowledged.length,
        received_through: receivedThrough,
      });
      dropLink();
      abort.abort();
      listeners.clear();
      errorListeners.clear();
    },
  };

  async function open(resume: TransportLinkResume | undefined, attempt: number) {
    const token = {};
    currentAttempt = token;
    const events: TransportLinkEvents = {
      frame(frame) {
        if (currentAttempt === token) receive(frame);
      },
      acknowledged(through) {
        if (currentAttempt === token) acknowledge(through);
      },
      closed(event) {
        if (currentAttempt === token) linkClosed(event.error);
      },
    };
    const startedAt = Date.now();
    logger.info("reliable_session_link_open_started", {
      ...connectionContext,
      resumed: Boolean(resume),
      attempt,
    });
    try {
      const result = await options.openLink({
        events,
        signal: abort.signal,
        ...(resume ? { resume } : {}),
      });
      if (closed || terminalError || currentAttempt !== token) {
        result.link.close();
        throw new Error("Reliable session channel closed while opening its link");
      }
      link = result.link;
      session = { sessionId: result.sessionId, serverId: result.serverId };
      acknowledge(result.peerReceivedThrough);
      logger.info("reliable_session_link_open_completed", {
        ...connectionContext,
        resumed: Boolean(resume),
        attempt,
        server_id: result.serverId,
        resent_frames: unacknowledged.length,
        duration_ms: Date.now() - startedAt,
      });
      // A link may deliver before its opener resolves; settle that cursor now.
      if (receivedThrough > (resume?.receivedThrough ?? 0)) result.link.acknowledge(receivedThrough);
      for (const frame of [...unacknowledged]) {
        if (link !== result.link) break;
        result.link.send(frame);
      }
      return result;
    } catch (error) {
      if (currentAttempt === token) currentAttempt = undefined;
      logger.warn("reliable_session_link_open_failed", {
        ...connectionContext,
        resumed: Boolean(resume),
        attempt,
        duration_ms: Date.now() - startedAt,
        ...diagnosticErrorFields(error),
      });
      throw error;
    }
  }

  function receive(frame: SequencedFrame) {
    if (frame.sequence <= receivedThrough) return;
    if (frame.sequence !== receivedThrough + 1) {
      // The link skipped a frame the session still holds for replay; a new
      // link resumes from the cursor instead of delivering out of order.
      logger.warn("reliable_session_sequence_gap", {
        ...connectionContext,
        expected_sequence: receivedThrough + 1,
        received_sequence: frame.sequence,
      });
      linkClosed(new Error(`App Server session sequence gap: expected ${receivedThrough + 1}`));
      return;
    }
    for (const listener of listeners) listener(frame.message);
    receivedThrough = frame.sequence;
    link?.acknowledge(receivedThrough);
  }

  function acknowledge(through: number) {
    while (unacknowledged[0] && unacknowledged[0].sequence <= through) unacknowledged.shift();
  }

  function linkClosed(error: unknown) {
    dropLink();
    if (closed || terminalError) return;
    if (transportLinkCloseKind(error) !== "interrupted" || !session) {
      fail(error);
      return;
    }
    logger.warn("reliable_session_link_interrupted", {
      ...connectionContext,
      unacknowledged_frames: unacknowledged.length,
      received_through: receivedThrough,
      ...diagnosticErrorFields(error),
    });
    void resume();
  }

  async function resume() {
    if (resuming) return;
    resuming = true;
    const startedAt = Date.now();
    let attempt = 0;
    try {
      // `link` is checked rather than the open result: a link can be lost
      // again while its unacknowledged tail is still being resent.
      while (!closed && !terminalError && !link && session) {
        attempt += 1;
        try {
          await open({ ...session, receivedThrough }, attempt);
        } catch (error) {
          if (closed || terminalError) return;
          if (transportLinkCloseKind(error) !== "interrupted") {
            fail(error);
            return;
          }
          const delayMs = Math.min(retryDelayMs * 2 ** Math.min(attempt - 1, 10), maxRetryDelayMs);
          logger.warn("reliable_session_resume_retry_scheduled", {
            ...connectionContext,
            attempt,
            retry_delay_ms: delayMs,
            duration_ms: Date.now() - startedAt,
          });
          await new Promise<void>((resolve) => setTimeout(resolve, delayMs));
        }
      }
      if (link) {
        logger.info("reliable_session_resumed", {
          ...connectionContext,
          attempt,
          duration_ms: Date.now() - startedAt,
        });
      }
    } finally {
      resuming = false;
    }
  }

  function dropLink() {
    const previous = link;
    link = undefined;
    currentAttempt = undefined;
    previous?.close();
  }

  function fail(error: unknown) {
    if (terminalError || closed) return;
    terminalError = error;
    logger.error("reliable_session_channel_failed", {
      ...connectionContext,
      link_close_kind: transportLinkCloseKind(error),
      ...diagnosticErrorFields(error),
    });
    dropLink();
    abort.abort();
    for (const listener of errorListeners) listener(error);
  }
}

function diagnosticErrorFields(error: unknown) {
  return {
    error_kind: error instanceof Error && error.name ? error.name : typeof error,
    ...transportLinkErrorDiagnosticFields(error),
  };
}
