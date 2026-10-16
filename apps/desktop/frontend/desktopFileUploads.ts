import type { FrontendFileAcquisition } from "../../../packages/frontend/src/services/frontendShell";
import { uploadFileOverHttp, type FileUploadLog } from "../../../packages/frontend/src/services/fileUploadHttp";
import { localHttpRouteUrl, type LocalHttpConnection } from "./desktopBootstrap";

/**
 * Copies attached files to the App Server currently in use, as the Web App does. Paths on this
 * computer cannot be referenced in place: the App Server may run in WSL or on a paired
 * computer, whose gateway carries these same routes. The endpoint is read per upload because
 * Desktop replaces it when the App Server restarts or the paired computer changes.
 */
export function createDesktopFileUploads(
  connection: () => LocalHttpConnection,
  clientInstanceId: string,
  log: FileUploadLog = logToConsole,
): FrontendFileAcquisition {
  return {
    kind: "webUpload",
    upload(taskId, file, onProgress, signal) {
      const { endpointUrl, authToken } = connection();
      const url = localHttpRouteUrl(endpointUrl, "upload");
      if (!url) {
        log("warn", "file_upload_completed", { outcome: "failure", error_kind: "endpoint", attempt_count: 0 });
        return Promise.reject(new Error("File upload is unavailable. Restart OpenAIDE and try again."));
      }
      return uploadFileOverHttp({
        target: { url: url.toString(), headers: { Authorization: `Bearer ${authToken}` } },
        taskId,
        file,
        clientInstanceId,
        onProgress,
        signal,
        log,
      });
    },
  };
}

/**
 * Native drop handling is off (`dragDropEnabled: false` in every Tauri window config) so the
 * shared Frontend receives dropped files as DOM `File`s. The webview would then replace the app
 * with a file dropped outside the Task surface, so this refuses every file drag nothing claimed.
 * Register it on the window: its listeners run after the Task surface's own.
 */
export function refuseUnclaimedFileDrops(target: Pick<Window, "addEventListener">) {
  const unclaimedFileDrag = (event: DragEvent) =>
    !event.defaultPrevented && Array.from(event.dataTransfer?.types ?? []).includes("Files");
  target.addEventListener("dragover", (event) => {
    if (!unclaimedFileDrag(event)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "none";
  });
  target.addEventListener("drop", (event) => {
    if (!unclaimedFileDrag(event)) return;
    event.preventDefault();
    console.info("desktop_file_drop_refused reason=outside_drop_target");
  });
}

function logToConsole(level: "info" | "warn", event: string, fields: Record<string, string | number>) {
  const pairs = Object.entries(fields).map(([name, value]) => `${name}=${value}`);
  console[level]([event, ...pairs].join(" "));
}
