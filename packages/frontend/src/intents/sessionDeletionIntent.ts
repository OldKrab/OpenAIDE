import {
  NATIVE_SESSION_DELETE,
  AppServerProtocolError,
  type BackendConnection,
  type ClientRequestId,
  type NativeSessionDeleteParams,
  type NativeSessionDeleteResult,
} from "@openaide/app-server-client";

export type DeleteSessionAction = (params: NativeSessionDeleteParams) => Promise<NativeSessionDeleteResult>;
const pending = new WeakMap<BackendConnection["request"], Set<string>>();

/** Destructive intent waits for Agent confirmation; it never optimistically removes history
 * or retries a lost response. The App Server owns confirmation rechecks and exclusion. */
export async function requestSessionDeletion(
  connection: Pick<BackendConnection, "request">,
  params: NativeSessionDeleteParams,
): Promise<NativeSessionDeleteResult> {
  const key = JSON.stringify(params.target);
  const requests = pending.get(connection.request) ?? new Set<string>();
  pending.set(connection.request, requests);
  if (requests.has(key)) throw new Error("Deletion is already in progress.");
  requests.add(key);
  const operationId = crypto.randomUUID() as ClientRequestId;
  const started = performance.now();
  console.info(`native_session_delete_started operation_id=${operationId} attempt=1`);
  try {
    const result = await connection.request(NATIVE_SESSION_DELETE, params, { clientRequestId: operationId });
    console.info(`native_session_delete_completed operation_id=${operationId} attempt=1 outcome=${result.kind} duration_ms=${Math.round(performance.now() - started)}`);
    return result;
  } catch (error) {
    const kind = error instanceof AppServerProtocolError ? error.protocolError.code : "transport";
    console.warn(`native_session_delete_completed operation_id=${operationId} attempt=1 outcome=failure error_kind=${kind} duration_ms=${Math.round(performance.now() - started)}`);
    throw error;
  } finally {
    requests.delete(key);
  }
}
