import type { BackendStateReset } from "./backendConnection.js";
import type { ProtocolMethod } from "./generated/protocol.js";

/**
 * Reports that a request lost its App Server replica while it was in flight.
 *
 * The original request may have reached the old process, so callers must
 * reconcile state before deciding whether to retry it against the new replica.
 */
export class BackendReplicaChangedError extends Error {
  readonly name = "BackendReplicaChangedError";

  constructor(
    readonly method: ProtocolMethod,
    readonly previousReplica: BackendStateReset | undefined,
    readonly currentReplica: BackendStateReset,
  ) {
    super(`App Server replica changed while ${method} was in flight`);
  }
}
