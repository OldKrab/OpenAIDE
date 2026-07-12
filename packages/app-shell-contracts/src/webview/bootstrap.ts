import type { AppPreferencesRecord } from "./preferences.js";
import type { SettingsTabId } from "./preferences.js";

export type WebviewSurfaceKind = "navigation" | "task" | "settings";

export type WebviewAppServerConnection = {
  kind: "localHttp";
  endpointUrl: string;
  authToken: string;
} | {
  kind: "webProxy";
  endpointUrl: string;
};

export type WebviewBootstrap = {
  surface: WebviewSurfaceKind;
  /** Host-issued identity unique to this webview instance. */
  clientInstanceId?: string;
  taskId?: string;
  projectId?: string;
  settingsTab?: SettingsTabId;
  preferences?: AppPreferencesRecord;
  appServerConnection?: WebviewAppServerConnection;
};
