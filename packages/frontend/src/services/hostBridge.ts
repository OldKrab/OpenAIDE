import {
  createReliableLocalHttpBackendConnection,
  createReliableWebProxyBackendConnection,
  type DiagnosticsLogger,
} from "@openaide/app-server-client";
import type {
  HostToWebviewMessage,
  SettingsTabId,
} from "@openaide/app-shell-contracts";
import { frontendShell } from "./frontendShell";
import type { PostHostMessage } from "../state/postHostMessage";
import { sendWebviewTelemetry } from "../state/hostMessageTelemetry";

/** Shared Frontend facade over the App Shell selected at the composition root. */
export function getBootstrap() {
  return frontendShell().bootstrap();
}

export function postHostMessage(message: Parameters<PostHostMessage>[0]) {
  frontendShell().messages.post(message);
}

export function subscribeHostMessages(listener: (message: HostToWebviewMessage) => void) {
  return frontendShell().messages.subscribe(listener);
}

export function subscribeSurfaceRouteChanges(listener: Parameters<ReturnType<typeof frontendShell>["navigation"]["subscribe"]>[0]) {
  return frontendShell().navigation.subscribe(listener);
}

export function openNewTaskSurface(projectId?: string) {
  frontendShell().navigation.openNewTask(projectId);
}

export function openNativeSessionSurface(agentId: string, nativeSessionId: string, projectId?: string) {
  frontendShell().navigation.openNativeSession(agentId, nativeSessionId, projectId);
}

export function openSettingsSurface(
  agentId?: string,
  returnToNewTask?: boolean,
  projectId?: string,
  settingsTab?: SettingsTabId,
) {
  frontendShell().navigation.openSettings(agentId, returnToNewTask, projectId, settingsTab);
}

export function openTaskSurface(taskId: string, title?: string, agentId?: string) {
  frontendShell().navigation.openTask(taskId, title, agentId);
}

export function updateTaskSurfaceTitle(taskId: string, title: string) {
  postHostMessage({
    type: "surface.updateTaskTitle",
    payload: { task_id: taskId, title },
  });
}

export function replaceSettingsTabRoute(tab: SettingsTabId) {
  frontendShell().navigation.replaceSettingsTab(tab);
}

export function openRecoveryUrl(url: string) {
  frontendShell().recovery.openExternal(url);
}

export function reloadRecoveryShell() {
  frontendShell().recovery.reload?.();
}

/** Returns the optional shell-owned action for recovering an empty workspace. */
export function getWorkspaceCapability() {
  return frontendShell().workspace;
}

export function getBackendConnection() {
  const shellConnection = frontendShell().backendConnection?.();
  if (shellConnection) return shellConnection;
  const bootstrap = getBootstrap();
  if (bootstrap.surface !== "invalid" && bootstrap.appServerConnection?.kind === "localHttp") {
    return createReliableLocalHttpBackendConnection({
      ...bootstrap.appServerConnection,
      connectionId: createTransportConnectionId(),
      logger: createFrontendDiagnosticsLogger(),
      subscribeToWake: subscribeToBrowserWake,
    });
  }
  if (bootstrap.surface !== "invalid" && bootstrap.appServerConnection?.kind === "webProxy") {
    return createReliableWebProxyBackendConnection({
      endpointUrl: bootstrap.appServerConnection.endpointUrl,
      connectionId: createTransportConnectionId(),
      logger: createFrontendDiagnosticsLogger(),
      subscribeToWake: subscribeToBrowserWake,
    });
  }
  return undefined;
}

function createFrontendDiagnosticsLogger(): DiagnosticsLogger {
  return {
    info: (event, fields = {}) => sendWebviewTelemetry(
      postHostMessage,
      `app_server_client_${event}`,
      fields,
    ),
    warn: (event, fields = {}) => sendWebviewTelemetry(
      postHostMessage,
      `app_server_client_${event}`,
      fields,
    ),
    error: (event, fields = {}) => sendWebviewTelemetry(
      postHostMessage,
      `app_server_client_${event}`,
      fields,
    ),
  };
}

/** Transport identity is disposable and must never double as the logical App Shell client. */
function createTransportConnectionId() {
  return `frontend-connection-${globalThis.crypto.randomUUID()}`;
}

/** Converts browser lifecycle restoration into a replayable transport receive wake-up. */
function subscribeToBrowserWake(wake: () => void) {
  let wasHidden = document.visibilityState === "hidden";
  const handleVisibilityChange = () => {
    const hidden = document.visibilityState === "hidden";
    if (wasHidden && !hidden) wake();
    wasHidden = hidden;
  };
  const handlePageShow = () => wake();
  const handleOnline = () => wake();
  document.addEventListener?.("visibilitychange", handleVisibilityChange);
  window.addEventListener?.("pageshow", handlePageShow);
  window.addEventListener?.("online", handleOnline);
  return () => {
    document.removeEventListener?.("visibilitychange", handleVisibilityChange);
    window.removeEventListener?.("pageshow", handlePageShow);
    window.removeEventListener?.("online", handleOnline);
  };
}
