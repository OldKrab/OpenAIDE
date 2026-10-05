import type { RpcMessage } from "./rpcPeer.js";

/** One message of a reliable session, numbered independently per direction. */
export type SequencedFrame = {
  sequence: number;
  message: RpcMessage;
};

/**
 * Why a link ended. Only `interrupted` leaves the session intact: the link
 * disappeared without a server decision, so the session may resume on a new
 * link. Every other kind is a definite outcome the connection layer handles.
 */
export type TransportLinkCloseKind =
  | "interrupted"
  | "sessionExpired"
  | "replayExpired"
  | "receiveStalled"
  | "rejected";

/** Callbacks a link drives; the reliable session owns ordering and replay. */
export type TransportLinkEvents = {
  /** A server frame arrived. Duplicates are allowed; the session drops them. */
  frame(frame: SequencedFrame): void;
  /** The server accepted every client frame up to and including `through`. */
  acknowledged(through: number): void;
  /** The link ended on its own. Never reported after a local `close()`. */
  closed(event: { kind: TransportLinkCloseKind; error: unknown }): void;
};

/**
 * A duplex path to one reliable session. It moves frames and acknowledgements
 * and nothing else: it may batch, push, or poll as its carrier allows, while
 * sequencing, deduplication, and resend stay in the session above it.
 */
export type TransportLink = {
  /** Sends one client frame. The session resends it if the link is lost first. */
  send(frame: SequencedFrame): void;
  /** Reports that every server frame through `through` was fully applied. */
  acknowledge(through: number): void;
  close(): void;
};

export type TransportLinkResume = {
  sessionId: string;
  serverId: string;
  /** Highest server sequence the client fully applied. */
  receivedThrough: number;
};

export type OpenedTransportLink = {
  link: TransportLink;
  sessionId: string;
  serverId: string;
  /** Highest client sequence the server holds; later frames must be resent. */
  peerReceivedThrough: number;
};

/** Opens a link to a new session, or to an existing one when `resume` is set. */
export type TransportLinkOpener = (request: {
  events: TransportLinkEvents;
  resume?: TransportLinkResume;
  signal: AbortSignal;
}) => Promise<OpenedTransportLink>;

/** A link failure classified for recovery and for metadata-only diagnostics. */
export class TransportLinkError extends Error {
  constructor(
    readonly kind: TransportLinkCloseKind,
    message: string,
  ) {
    super(message);
  }

  /** Support Export-safe facts: no body, endpoint, identity, or credential. */
  diagnosticFields(): Record<string, unknown> {
    return { error_kind: "transport_link", link_close_kind: this.kind };
  }
}

/** Classifies any failure; an unclassified one is treated as a lost link. */
export function transportLinkCloseKind(error: unknown): TransportLinkCloseKind {
  return error instanceof TransportLinkError ? error.kind : "interrupted";
}

export function transportLinkErrorDiagnosticFields(error: unknown): Record<string, unknown> {
  return error instanceof TransportLinkError ? error.diagnosticFields() : {};
}

export function rpcMethod(message: RpcMessage) {
  return "method" in message && typeof message.method === "string"
    ? message.method
    : "response";
}
