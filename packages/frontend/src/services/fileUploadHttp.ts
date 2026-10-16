import type { PreSendAttachment } from "@openaide/app-server-client";
import type { FileUploadProgress } from "./frontendShell";

const MAX_CHUNK_BYTES = 64 * 1024;
const CHUNK_REQUEST_TIMEOUT_MS = 5 * 60_000;
const chunkPreferredFiles = new WeakSet<File>();

/**
 * Where a shell reaches the App Server upload route. Web goes through its same-origin proxy,
 * which supplies credentials itself; Desktop calls the App Server endpoint in use with its
 * bearer token, which also carries uploads to a paired computer through its gateway.
 */
export type FileUploadTarget = {
  /** The single-request route; chunks go to `${url}/chunk`. */
  url: string;
  /** Credentials the browser does not attach on its own. */
  headers?: Record<string, string>;
};

/** Receives metadata-only diagnostics: no file names, content, or error messages. */
export type FileUploadLog = (
  level: "info" | "warn",
  event: string,
  fields: Record<string, string | number>,
) => void;

export type FileUploadRequest = {
  target: FileUploadTarget;
  taskId: string;
  file: File;
  clientInstanceId: string;
  onProgress: (progress: FileUploadProgress) => void;
  signal: AbortSignal;
  log?: FileUploadLog;
};

type UploadErrorKind =
  | "requestTooLarge"
  | "transportError"
  | "intermediaryRejected"
  | "timeout"
  | "rejected"
  | "invalidResponse";
const FALLBACK_KINDS = new Set<UploadErrorKind>(["requestTooLarge", "transportError", "intermediaryRejected"]);

class UploadError extends Error {
  constructor(message: string, readonly kind: UploadErrorKind, readonly status?: number) {
    super(message);
  }
}

/** Uploads through the fast single-request path, then falls back to chunks for request-size failures. */
export async function uploadFileOverHttp(request: FileUploadRequest): Promise<PreSendAttachment> {
  const { file, log } = request;
  const operationId = newUploadId();
  const startedAt = performance.now();
  let mode: "single" | "chunked" = chunkPreferredFiles.has(file) ? "chunked" : "single";
  let attemptCount = 1;
  log?.("info", "file_upload_started", { operation_id: operationId, mode, bytes: file.size });
  const finished = (outcome: string) => ({
    operation_id: operationId,
    outcome,
    mode,
    attempt_count: attemptCount,
    duration_ms: Math.round(performance.now() - startedAt),
  });
  try {
    let attachment: PreSendAttachment;
    if (mode === "chunked") attachment = await uploadFileInChunks(request);
    else {
      try {
        attachment = await uploadFileOnce(request);
      } catch (error) {
        if (!(error instanceof UploadError) || !FALLBACK_KINDS.has(error.kind)) throw error;
        log?.("info", "file_upload_chunk_fallback", { operation_id: operationId, reason: error.kind });
        chunkPreferredFiles.add(file);
        mode = "chunked";
        attemptCount = 2;
        attachment = await uploadFileInChunks(request);
      }
    }
    log?.("info", "file_upload_completed", finished("success"));
    return attachment;
  } catch (error) {
    if (isAbort(error)) log?.("info", "file_upload_completed", finished("cancelled"));
    else {
      log?.("warn", "file_upload_completed", {
        ...finished("failure"),
        error_kind: error instanceof UploadError ? error.kind : "unexpected",
        ...(error instanceof UploadError && error.status !== undefined ? { status: error.status } : {}),
      });
    }
    throw error;
  }
}

async function uploadFileOnce({ target, taskId, file, clientInstanceId, onProgress, signal }: FileUploadRequest) {
  const response = await sendUploadRequest({
    url: target.url,
    headers: { ...target.headers, ...uploadHeaders(taskId, file, clientInstanceId) },
    body: file,
    signal,
    onProgress: (loaded, total) => onProgress({
      loaded,
      total: total ?? file.size,
    }),
  });
  if (response.status === 413 || response.status === 0) {
    throw new UploadError(uploadErrorMessage(response.body), "requestTooLarge", response.status);
  }
  // Some enterprise gateways mask request-body limits as branded HTML 403 pages.
  // App Server 403 JSON responses remain authoritative and must not be retried.
  if (response.status === 403 && isHtmlResponse(response.contentType)) {
    throw new UploadError(uploadErrorMessage(response.body), "intermediaryRejected", response.status);
  }
  if (response.status !== 200) throw new UploadError(uploadErrorMessage(response.body), "rejected", response.status);
  return attachmentFromResponse(response.body);
}

async function uploadFileInChunks({ target, taskId, file, clientInstanceId, onProgress, signal }: FileUploadRequest) {
  const uploadId = newUploadId();
  let offset = 0;
  let sentChunk = false;
  try {
    while (offset < file.size || !sentChunk) {
      const end = Math.min(offset + MAX_CHUNK_BYTES, file.size);
      const chunk = file.slice(offset, end);
      const chunkOffset = offset;
      const response = await sendUploadRequest({
        url: chunkUrl(target),
        headers: {
          ...target.headers,
          ...uploadHeaders(taskId, file, clientInstanceId),
          "X-OpenAIDE-Upload-Id": uploadId,
          "X-OpenAIDE-Upload-Offset": String(chunkOffset),
          "X-OpenAIDE-Upload-Size": String(file.size),
        },
        body: chunk,
        signal,
        timeoutMs: CHUNK_REQUEST_TIMEOUT_MS,
        onProgress: (loaded) => onProgress({
          loaded: chunkOffset + loaded,
          total: file.size,
        }),
      });
      sentChunk = true;
      if (response.status === 200) {
        const attachment = attachmentFromResponse(response.body);
        chunkPreferredFiles.delete(file);
        return attachment;
      }
      if (response.status !== 202) throw new UploadError(uploadErrorMessage(response.body), "rejected", response.status);
      offset = end;
      if (signal.aborted) throw new DOMException("Upload cancelled", "AbortError");
    }
    throw new UploadError("Chunked upload completed without an attachment.", "invalidResponse");
  } catch (error) {
    void cancelChunkUpload(target, clientInstanceId, uploadId);
    throw error;
  }
}

function sendUploadRequest({
  url,
  headers,
  body,
  signal,
  timeoutMs,
  onProgress,
}: {
  url: string;
  headers: Record<string, string>;
  body: Blob;
  signal: AbortSignal;
  timeoutMs?: number;
  onProgress?: (loaded: number, total?: number) => void;
}): Promise<{ status: number; body: string; contentType: string | null }> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) {
      reject(new DOMException("Upload cancelled", "AbortError"));
      return;
    }
    const request = new XMLHttpRequest();
    const abort = () => request.abort();
    const finish = (settle: () => void) => {
      signal.removeEventListener("abort", abort);
      settle();
    };
    signal.addEventListener("abort", abort, { once: true });
    request.open("POST", url);
    if (timeoutMs !== undefined) request.timeout = timeoutMs;
    for (const [name, value] of Object.entries(headers)) request.setRequestHeader(name, value);
    request.upload.addEventListener("progress", (event) => {
      onProgress?.(event.loaded, event.lengthComputable ? event.total : undefined);
    });
    request.addEventListener("load", () => finish(() => resolve({
      status: request.status,
      body: request.responseText,
      contentType: request.getResponseHeader("Content-Type"),
    })));
    request.addEventListener("error", () => finish(() => reject(
      new UploadError("File upload failed.", "transportError"),
    )));
    request.addEventListener("timeout", () => finish(() => reject(
      new UploadError("File upload timed out. Retry the upload.", "timeout"),
    )));
    request.addEventListener("abort", () => finish(() => reject(new DOMException("Upload cancelled", "AbortError"))));
    request.send(body);
  });
}

function isHtmlResponse(contentType: string | null) {
  return contentType?.split(";", 1)[0].trim().toLowerCase() === "text/html";
}

function isAbort(error: unknown) {
  return error instanceof DOMException && error.name === "AbortError";
}

function chunkUrl(target: FileUploadTarget) {
  return `${target.url}/chunk`;
}

function cancelChunkUpload(target: FileUploadTarget, clientInstanceId: string, uploadId: string) {
  const request = new XMLHttpRequest();
  request.open("POST", chunkUrl(target));
  for (const [name, value] of Object.entries(target.headers ?? {})) request.setRequestHeader(name, value);
  request.setRequestHeader("Content-Type", "application/octet-stream");
  request.setRequestHeader("X-OpenAIDE-Client-Instance-Id", clientInstanceId);
  request.setRequestHeader("X-OpenAIDE-Upload-Id", uploadId);
  request.setRequestHeader("X-OpenAIDE-Upload-Cancel", "true");
  request.send(new Blob());
}

function uploadHeaders(
  taskId: string,
  file: File,
  clientInstanceId: string,
) {
  return {
    "Content-Type": "application/octet-stream",
    "X-OpenAIDE-Client-Instance-Id": clientInstanceId,
    "X-OpenAIDE-Task-Id": taskId,
    "X-OpenAIDE-File-Name": encodeURIComponent(file.name || "Attached file"),
  };
}

function attachmentFromResponse(responseText: string) {
  let value: { attachment?: PreSendAttachment } | null;
  try {
    value = JSON.parse(responseText) as { attachment?: PreSendAttachment } | null;
  } catch {
    throw new UploadError("Upload response did not include an attachment.", "invalidResponse");
  }
  if (!value?.attachment) throw new UploadError("Upload response did not include an attachment.", "invalidResponse");
  return value.attachment;
}

function uploadErrorMessage(responseText: string) {
  try {
    const value = JSON.parse(responseText) as { error?: { message?: string } };
    return value.error?.message || "File upload failed.";
  } catch {
    return "File upload failed.";
  }
}

function newUploadId() {
  return globalThis.crypto?.randomUUID?.() ?? `upload-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
