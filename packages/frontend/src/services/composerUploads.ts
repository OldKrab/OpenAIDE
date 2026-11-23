import type { BackendConnection, PreSendAttachment } from "@openaide/app-server-client";
import { uploadFailureFields, type FileUploadKind } from "./fileUploadHttp";
import type { FileUploadProgress, FrontendFileAcquisition } from "./frontendShell";
import { recordFrontendLifecycleEvent } from "./frontendLifecycleTelemetry";

type WebUpload = Extract<FrontendFileAcquisition, { kind: "webUpload" }>;

/**
 * Uploads one Composer file or Image through the App Shell and records its start and end in
 * the App Server log (`frontend_composer_upload_*`), beside the App Server's own
 * `attachment_upload_*` events. The shell's transport log stays in its own telemetry.
 */
export async function uploadComposerFile(
  files: WebUpload,
  /** The Task's stable connection: its diagnostics forwarder is kept per connection object. */
  backendConnection: Partial<Pick<BackendConnection, "request">> | undefined,
  taskId: string,
  file: File,
  options: { kind?: FileUploadKind; onProgress: (progress: FileUploadProgress) => void; signal: AbortSignal },
): Promise<PreSendAttachment> {
  const kind = options.kind ?? "file";
  const startedAt = performance.now();
  const connection = backendConnection?.request ? backendConnection as Pick<BackendConnection, "request"> : undefined;
  const record = (event: string, fields: Record<string, string | number>) =>
    recordFrontendLifecycleEvent(connection, event, { task_id: taskId, kind, bytes: file.size, ...fields });
  record("composer_upload_started", {});
  try {
    const attachment = await files.upload(taskId, file, options.onProgress, options.signal, { kind });
    record("composer_upload_completed", { outcome: "success", duration_ms: performance.now() - startedAt });
    return attachment;
  } catch (error) {
    const cancelled = options.signal.aborted;
    record("composer_upload_completed", {
      outcome: cancelled ? "cancelled" : "failure",
      duration_ms: performance.now() - startedAt,
      // The transport's class and status show where a failure stopped: refused by the App Server
      // or a gateway (`rejected` with its status), or never answered (`transportError`, `timeout`).
      ...(cancelled ? {} : uploadFailureFields(error)),
    });
    throw error;
  }
}
