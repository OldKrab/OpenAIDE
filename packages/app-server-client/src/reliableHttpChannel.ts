import type { RpcMessage, RpcMessageChannel } from "./rpcPeer.js";

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
};

type SessionHandshake = {
  transportVersion: 1;
  sessionId: string;
  serverId: string;
};

type ServerBatch = {
  frames: Array<{ sequence: number; message: RpcMessage }>;
};

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
  const listeners = new Set<(message: RpcMessage) => void>();
  const errorListeners = new Set<(error: unknown) => void>();
  const uploads: Array<{ sequence: number; message: RpcMessage; body: string }> = [];
  const abort = new AbortController();
  const retryDelayMs = options.retryDelayMs ?? 250;
  let nextClientSequence = 1;
  let lastServerSequence = 0;
  let pumping = false;
  let closed = false;
  let terminalError: unknown;
  const session = openSession().catch((error) => {
    fail(error);
    throw error;
  });
  void receiveLoop();

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
      abort.abort();
      listeners.clear();
      errorListeners.clear();
    },
  };

  async function openSession(): Promise<SessionHandshake> {
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
    return handshake;
  }

  async function pumpUploads() {
    if (pumping || closed) return;
    pumping = true;
    try {
      const opened = await session;
      while (!closed && uploads.length > 0) {
        const upload = uploads[0];
        if (!upload) break;
        const body = upload.body.replace("__SESSION_ID__", opened.sessionId);
        try {
          const response = await fetchImpl(options.endpointUrl, {
            method: "POST",
            headers: baseHeaders(),
            body,
            signal: abort.signal,
          });
          const text = await response.text();
          if (!response.ok) throw httpError("upload", response.status, text);
          uploads.shift();
        } catch (error) {
          if (closed || isAbort(error)) return;
          if (isTerminalHttpError(error)) {
            fail(error);
            return;
          }
          await retryDelay();
        }
      }
    } finally {
      pumping = false;
      if (!closed && !terminalError && uploads.length > 0) void pumpUploads();
    }
  }

  async function receiveLoop() {
    let opened: SessionHandshake;
    try {
      opened = await session;
    } catch {
      return;
    }
    while (!closed) {
      try {
        const response = await fetchImpl(options.endpointUrl, {
          method: "GET",
          headers: {
            ...baseHeaders(),
            "X-OpenAIDE-Session-Id": opened.sessionId,
            "X-OpenAIDE-After": String(lastServerSequence),
          },
          signal: abort.signal,
        });
        const text = await response.text();
        if (response.status === 204) {
          // Real polls are held by the server. Yield here as well so a test
          // double or intermediary returning immediately cannot spin the UI.
          await retryDelay();
          continue;
        }
        if (!response.ok) throw httpError("receive", response.status, text);
        const batch = JSON.parse(text) as ServerBatch;
        for (const frame of batch.frames) {
          if (frame.sequence <= lastServerSequence) continue;
          if (frame.sequence !== lastServerSequence + 1) {
            throw new Error(`App Server session sequence gap: expected ${lastServerSequence + 1}`);
          }
          for (const listener of listeners) listener(frame.message);
          lastServerSequence = frame.sequence;
        }
      } catch (error) {
        if (closed || isAbort(error)) return;
        if (isTerminalHttpError(error)) {
          fail(error);
          return;
        }
        await retryDelay();
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
    abort.abort();
    for (const listener of errorListeners) listener(error);
  }
}

function httpError(operation: string, status: number, body: string) {
  return new ReliableHttpError(operation, status, body);
}

class ReliableHttpError extends Error {
  constructor(operation: string, readonly status: number, body: string) {
    super(`App Server reliable-session ${operation} failed with HTTP ${status}: ${body}`);
  }
}

/** A gone session is safe to replace, but the interrupted RPC is still ambiguous. */
export function isReliableHttpSessionExpired(error: unknown) {
  return error instanceof ReliableHttpError && error.status === 410;
}

function isTerminalHttpError(error: unknown) {
  return error instanceof ReliableHttpError
    && [400, 401, 403, 409, 410].includes(error.status);
}

function isAbort(error: unknown) {
  return error instanceof Error && error.name === "AbortError";
}
