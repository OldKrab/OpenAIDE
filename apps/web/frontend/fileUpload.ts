import type { PreSendAttachment } from "@openaide/app-server-client";

const UPLOAD_PATH = "/__openaide-app-server/upload";
const CHUNK_UPLOAD_PATH = `${UPLOAD_PATH}/chunk`;
const MAX_CHUNK_BYTES = 512 * 1024;
const chunkPreferredFiles = new WeakSet<File>();

type UploadProgress = { loaded: number; total: number };
type UploadFallbackReason = "requestTooLarge" | "transportError" | "intermediaryRejected";
export type UploadAttachmentMetadata = {
  kind: "image";
  mimeType: string;
};

class UploadFallbackError extends Error {
  constructor(message: string, readonly reason: UploadFallbackReason) {
    super(message);
  }
}

/** Uploads through the fast single-request path, then falls back for request-size failures. */
export async function uploadFile(
  taskId: string,
  file: File,
  clientInstanceId: string,
  onProgress: (progress: UploadProgress) => void,
  signal: AbortSignal,
  onFallback?: (reason: UploadFallbackReason) => void,
  metadata?: UploadAttachmentMetadata,
): Promise<PreSendAttachment> {
  if (chunkPreferredFiles.has(file)) {
    return uploadFileInChunks(taskId, file, clientInstanceId, onProgress, signal, metadata);
  }
  try {
    return await uploadFileOnce(taskId, file, clientInstanceId, onProgress, signal, metadata);
  } catch (error) {
    if (!(error instanceof UploadFallbackError)) throw error;
    onFallback?.(error.reason);
    chunkPreferredFiles.add(file);
    return uploadFileInChunks(taskId, file, clientInstanceId, onProgress, signal, metadata);
  }
}

async function uploadFileOnce(
  taskId: string,
  file: File,
  clientInstanceId: string,
  onProgress: (progress: UploadProgress) => void,
  signal: AbortSignal,
  metadata?: UploadAttachmentMetadata,
) {
  const response = await sendUploadRequest({
    path: UPLOAD_PATH,
    headers: uploadHeaders(taskId, file, clientInstanceId, metadata),
    body: file,
    signal,
    onProgress: (loaded, total) => onProgress({
      loaded,
      total: total ?? file.size,
    }),
  });
  if (response.status === 413 || response.status === 0) {
    throw new UploadFallbackError(uploadErrorMessage(response.body), "requestTooLarge");
  }
  // Some enterprise gateways mask request-body limits as branded HTML 403 pages.
  // App Server 403 JSON responses remain authoritative and must not be retried.
  if (response.status === 403 && isHtmlResponse(response.contentType)) {
    throw new UploadFallbackError(uploadErrorMessage(response.body), "intermediaryRejected");
  }
  if (response.status !== 200) throw new Error(uploadErrorMessage(response.body));
  return attachmentFromResponse(response.body);
}

async function uploadFileInChunks(
  taskId: string,
  file: File,
  clientInstanceId: string,
  onProgress: (progress: UploadProgress) => void,
  signal: AbortSignal,
  metadata?: UploadAttachmentMetadata,
) {
  const uploadId = newUploadId();
  let offset = 0;
  let sentChunk = false;
  try {
    while (offset < file.size || !sentChunk) {
      const end = Math.min(offset + MAX_CHUNK_BYTES, file.size);
      const chunk = file.slice(offset, end);
      const chunkOffset = offset;
      const response = await sendUploadRequest({
        path: CHUNK_UPLOAD_PATH,
        headers: {
          ...uploadHeaders(taskId, file, clientInstanceId, metadata),
          "X-OpenAIDE-Upload-Id": uploadId,
          "X-OpenAIDE-Upload-Offset": String(chunkOffset),
          "X-OpenAIDE-Upload-Size": String(file.size),
        },
        body: chunk,
        signal,
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
      if (response.status !== 202) throw new Error(uploadErrorMessage(response.body));
      offset = end;
    }
    throw new Error("Chunked upload completed without an attachment.");
  } catch (error) {
    void cancelChunkUpload(clientInstanceId, uploadId);
    throw error;
  }
}

function sendUploadRequest({
  path,
  headers,
  body,
  signal,
  onProgress,
}: {
  path: string;
  headers: Record<string, string>;
  body: Blob;
  signal: AbortSignal;
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
    request.open("POST", path);
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
      new UploadFallbackError("File upload failed.", "transportError"),
    )));
    request.addEventListener("abort", () => finish(() => reject(new DOMException("Upload cancelled", "AbortError"))));
    request.send(body);
  });
}

function isHtmlResponse(contentType: string | null) {
  return contentType?.split(";", 1)[0].trim().toLowerCase() === "text/html";
}

function cancelChunkUpload(clientInstanceId: string, uploadId: string) {
  const request = new XMLHttpRequest();
  request.open("POST", CHUNK_UPLOAD_PATH);
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
  metadata?: UploadAttachmentMetadata,
) {
  return {
    "Content-Type": "application/octet-stream",
    "X-OpenAIDE-Client-Instance-Id": clientInstanceId,
    "X-OpenAIDE-Task-Id": taskId,
    "X-OpenAIDE-File-Name": encodeURIComponent(file.name || "Attached file"),
    ...(metadata ? {
      "X-OpenAIDE-Attachment-Kind": metadata.kind,
      "X-OpenAIDE-Mime-Type": metadata.mimeType,
    } : {}),
  };
}

function attachmentFromResponse(responseText: string) {
  const value = JSON.parse(responseText) as { attachment?: PreSendAttachment };
  if (!value.attachment) throw new Error("Upload response did not include an attachment.");
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
