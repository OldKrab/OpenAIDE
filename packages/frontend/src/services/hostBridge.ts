import {
  createReliableLocalHttpBackendConnection,
  createReliableWebProxyBackendConnection,
} from "@openaide/app-server-client";
import type {
  HostToWebviewMessage,
  SettingsTabId,
} from "@openaide/app-shell-contracts";
import { frontendShell } from "./frontendShell";
import type { PostHostMessage } from "../state/postHostMessage";

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

export function openSettingsSurface(agentId?: string, returnToNewTask?: boolean, projectId?: string) {
  frontendShell().navigation.openSettings(agentId, returnToNewTask, projectId);
}

export function openTaskSurface(taskId: string, title?: string) {
  frontendShell().navigation.openTask(taskId, title);
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

export function getBackendConnection() {
  const shellConnection = frontendShell().backendConnection?.();
  if (shellConnection) return shellConnection;
  const bootstrap = getBootstrap();
  if (bootstrap.surface !== "invalid" && bootstrap.appServerConnection?.kind === "localHttp") {
    return createReliableLocalHttpBackendConnection({
      ...bootstrap.appServerConnection,
      connectionId: createTransportConnectionId(),
      subscribeToWake: subscribeToBrowserWake,
    });
  }
  if (bootstrap.surface !== "invalid" && bootstrap.appServerConnection?.kind === "webProxy") {
    return createReliableWebProxyBackendConnection({
      endpointUrl: bootstrap.appServerConnection.endpointUrl,
      connectionId: createTransportConnectionId(),
      subscribeToWake: subscribeToBrowserWake,
    });
  }
  return undefined;
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
