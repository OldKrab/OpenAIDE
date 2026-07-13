import {
  createLocalHttpBackendConnection,
  createWebProxyBackendConnection,
} from "@openaide/app-server-client";
import type {
  HostToWebviewMessage,
  SettingsTabId,
} from "@openaide/app-shell-contracts";
import { clientInstanceIdForBootstrap } from "./backendInitialization";
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

export function openSettingsSurface() {
  frontendShell().navigation.openSettings();
}

export function openTaskSurface(taskId: string, title?: string) {
  frontendShell().navigation.openTask(taskId, title);
}

export function replaceSettingsTabRoute(tab: SettingsTabId) {
  frontendShell().navigation.replaceSettingsTab(tab);
}

export function getBackendConnection() {
  const bootstrap = getBootstrap();
  if (bootstrap.surface !== "invalid" && bootstrap.appServerConnection?.kind === "localHttp") {
    return createLocalHttpBackendConnection({
      ...bootstrap.appServerConnection,
      connectionId: clientInstanceIdForBootstrap(bootstrap),
    });
  }
  if (bootstrap.surface !== "invalid" && bootstrap.appServerConnection?.kind === "webProxy") {
    return createWebProxyBackendConnection({
      endpointUrl: bootstrap.appServerConnection.endpointUrl,
      connectionId: clientInstanceIdForBootstrap(bootstrap),
    });
  }
  return undefined;
}
