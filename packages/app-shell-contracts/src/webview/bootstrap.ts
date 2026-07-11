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
  taskId?: string;
  projectId?: string;
  settingsTab?: SettingsTabId;
  preferences?: AppPreferencesRecord;
  appServerConnection?: WebviewAppServerConnection;
};
