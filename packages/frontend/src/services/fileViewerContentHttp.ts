import type { FileViewerContentResult } from "./frontendShell";

/**
 * Reads preview bytes from the App Server's viewer download route. Shells differ only in
 * how they reach and authenticate to it: Web through its same-origin proxy cookie, Desktop
 * with the bearer token of its current App Server endpoint.
 */
export async function readFileViewerContentOverHttp(
  url: string,
  init: Omit<RequestInit, "cache" | "signal">,
  maxBytes: number,
  signal: AbortSignal,
): Promise<FileViewerContentResult> {
  const response = await fetch(url, { ...init, cache: "no-store", signal });
  if (response.status !== 200) {
    await response.body?.cancel();
    switch (response.status) {
      case 400: return { kind: "notAFile" };
      case 401:
      case 403: return { kind: "permissionDenied" };
      case 404: return { kind: "notFound" };
      default: return { kind: "unavailable" };
    }
  }
  // The endpoint always declares the length; refuse an oversized file before buffering it.
  const declared = Number(response.headers.get("content-length") ?? Number.NaN);
  if (!Number.isFinite(declared) || declared > maxBytes) {
    await response.body?.cancel();
    return { kind: Number.isFinite(declared) ? "tooLarge" : "unavailable" };
  }
  const bytes = await response.arrayBuffer();
  return bytes.byteLength > maxBytes ? { kind: "tooLarge" } : { kind: "bytes", bytes };
}
