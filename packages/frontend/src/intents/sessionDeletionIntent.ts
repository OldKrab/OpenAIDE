import {
  NATIVE_SESSION_DELETE,
  type BackendConnection,
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
  try {
    return await connection.request(NATIVE_SESSION_DELETE, params);
  } finally {
    requests.delete(key);
  }
}
