import type { HostToWebviewMessage, WebviewAppServerConnection } from "@openaide/app-shell-contracts";
import type { WebviewBootstrap } from "../state/surfaceTypes";

export function datasetBootstrap(): WebviewBootstrap {
  const surface = document.body.dataset.surface;
  if (surface !== "navigation" && surface !== "settings" && surface !== "task") {
    return { surface: "invalid" };
  }
  return {
    surface,
    clientInstanceId: document.body.dataset.clientInstanceId || undefined,
    taskId: document.body.dataset.taskId || undefined,
    projectId: document.body.dataset.projectId || undefined,
    preferences: shellPreferences(),
    appServerConnection: appServerConnection(),
  };
}

export function appServerConnection(): WebviewAppServerConnection | undefined {
  const value = document.body.dataset.appServerConnection;
  if (!value) return undefined;
  try {
    const record = JSON.parse(value) as Record<string, unknown>;
    if (record.kind === "localHttp" && typeof record.endpointUrl === "string" && typeof record.authToken === "string") {
      return { kind: "localHttp", endpointUrl: record.endpointUrl, authToken: record.authToken };
    }
    if (record.kind === "webProxy" && typeof record.endpointUrl === "string") {
      return { kind: "webProxy", endpointUrl: record.endpointUrl };
    }
  } catch {
    return undefined;
  }
  return undefined;
}

export function shellPreferences() {
  return {
    composer_submit_shortcut:
      document.body.dataset.composerSubmitShortcut === "enter" ? "enter" : "mod_enter",
  } as const;
}

export function subscribeWindowMessages(listener: (message: HostToWebviewMessage) => void) {
  const onMessage = (event: MessageEvent) => listener(event.data);
  window.addEventListener("message", onMessage);
  return () => window.removeEventListener("message", onMessage);
}
