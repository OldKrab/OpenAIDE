import {
  AppServerProtocolError,
  TASK_IMAGE,
  type BackendConnection,
  type TaskId,
  type TaskImageVariant,
} from "@openaide/app-server-client";
import type { ClientDiagnosticFields } from "./clientDiagnosticsIntents";

type ChatImageConnection = Pick<BackendConnection, "request">;

export type ChatImageLoadEvent =
  | "chat_image_load_started"
  | "chat_image_load_completed"
  | "chat_image_load_failed";

/**
 * One loaded copy of a stored image. `mediaType` is set only when App Server sent a
 * re-encoded preview; otherwise the bytes have the reference's media type.
 */
export type LoadedChatImage = { data: string; mediaType?: string };

/** Receives metadata only: ids, sizes, durations, and error classes, never bytes or messages. */
export type ChatImageLoadLog = (event: ChatImageLoadEvent, fields: ClientDiagnosticFields) => void;

/**
 * Reads one stored image of one Task (`task/image`, ADR-0075). A Chat row asks for the
 * `preview` and the full-size view for the `original` (ADR-0084). Each call is one App
 * Server request; `chatImageCache` decides when a request is needed, so cache hits never
 * reach this log.
 */
export async function requestChatImage(
  connection: ChatImageConnection,
  taskId: string,
  imageId: string,
  variant: TaskImageVariant,
  log: ChatImageLoadLog,
  now: () => number = () => performance.now(),
): Promise<LoadedChatImage> {
  const startedAt = now();
  const fields = { task_id: taskId, image_id: imageId, variant, cache_hit: false };
  log("chat_image_load_started", fields);
  try {
    const result = await connection.request(TASK_IMAGE, { taskId: taskId as TaskId, imageId, variant });
    log("chat_image_load_completed", {
      ...fields,
      duration_ms: now() - startedAt,
      size_bytes: base64DecodedBytes(result.data),
      reduced: result.mediaType !== undefined,
    });
    return result.mediaType === undefined ? { data: result.data } : { data: result.data, mediaType: result.mediaType };
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
