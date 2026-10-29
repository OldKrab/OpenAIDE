import {
  AppServerProtocolError,
  TASK_IMAGE,
  type BackendConnection,
  type TaskId,
} from "@openaide/app-server-client";
import type { ClientDiagnosticFields } from "./clientDiagnosticsIntents";

type ChatImageConnection = Pick<BackendConnection, "request">;

export type ChatImageLoadEvent =
  | "chat_image_load_started"
  | "chat_image_load_completed"
  | "chat_image_load_failed";

/** Receives metadata only: ids, sizes, durations, and error classes, never bytes or messages. */
export type ChatImageLoadLog = (event: ChatImageLoadEvent, fields: ClientDiagnosticFields) => void;

/**
 * Reads one stored image of one Task (`task/image`, ADR-0075) and returns its base64 bytes.
 * The caller holds the reference and so the media type. Each call is one App Server request;
 * `chatImageCache` decides when a request is needed, so cache hits never reach this log.
 */
export async function requestChatImage(
  connection: ChatImageConnection,
  taskId: string,
  imageId: string,
  log: ChatImageLoadLog,
  now: () => number = () => performance.now(),
): Promise<string> {
  const startedAt = now();
  const fields = { task_id: taskId, image_id: imageId, cache_hit: false };
  log("chat_image_load_started", fields);
  try {
    const result = await connection.request(TASK_IMAGE, { taskId: taskId as TaskId, imageId });
    log("chat_image_load_completed", {
      ...fields,
      duration_ms: now() - startedAt,
      size_bytes: base64DecodedBytes(result.data),
    });
    return result.data;
  } catch (error) {
    log("chat_image_load_failed", {
      ...fields,
      duration_ms: now() - startedAt,
      error_name: error instanceof Error ? error.name : typeof error,
      error_code: error instanceof AppServerProtocolError ? error.protocolError.code : undefined,
    });
    throw error;
  }
}

function base64DecodedBytes(data: string) {
  const padding = data.endsWith("==") ? 2 : data.endsWith("=") ? 1 : 0;
  return Math.max(0, Math.floor((data.length * 3) / 4) - padding);
}
