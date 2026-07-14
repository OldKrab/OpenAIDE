import { sendWebviewTelemetry } from "../state/hostMessageRouter";
import type { PostHostMessage } from "../state/postHostMessage";
import { shouldRequestWorkspaceRoots } from "../state/surfaceRouting";
import type { WebviewSurface } from "../state/surfaceTypes";

type StartupSurface = { surface: WebviewSurface; taskId?: string };

export function postControllerStarted(
  postHostMessage: PostHostMessage,
  bootstrap: StartupSurface,
) {
  sendWebviewTelemetry(postHostMessage, "started", {
    surface: bootstrap.surface,
    task_id: bootstrap.taskId,
  });
}

export function postStartupRequests({
  bootstrap,
  dispatchNavigationError,
  dispatchSettingsStart,
  dispatchSettingsError,
  dispatchTaskOpenError,
  skipTaskReadRequests = false,
  skipSettingsReadRequests = false,
  postHostMessage,
}: {
  bootstrap: StartupSurface;
  dispatchNavigationError: (message: string) => void;
  dispatchSettingsStart: () => void;
  dispatchSettingsError: (message: string) => void;
  dispatchTaskOpenError: (taskId: string, message: string) => void;
  skipTaskReadRequests?: boolean;
  skipSettingsReadRequests?: boolean;
  postHostMessage: PostHostMessage;
}) {
  if (shouldRequestWorkspaceRoots(bootstrap)) {
    postHostMessage({ type: "workspace.roots" });
  }
  if (!skipTaskReadRequests && bootstrap.surface === "navigation") {
    dispatchNavigationError("App Server connection unavailable.");
  }
  if (!skipTaskReadRequests && bootstrap.surface === "task" && bootstrap.taskId) {
    dispatchTaskOpenError(bootstrap.taskId, "App Server connection unavailable.");
  }
  if (!skipSettingsReadRequests && bootstrap.surface === "settings") {
    dispatchSettingsStart();
    dispatchSettingsError("App Server connection unavailable.");
  }
}
