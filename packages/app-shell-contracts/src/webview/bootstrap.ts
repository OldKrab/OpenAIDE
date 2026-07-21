import type { AppPreferencesRecord } from "./preferences.js";
import type { SettingsTabId } from "./preferences.js";

export type WebviewSurfaceKind = "navigation" | "nativeSession" | "task" | "settings";

export type AppShellBootstrap = {
  kind: "web" | "vscodeExtension";
  /** Selects Project Navigation or current-Project Task Navigation independently of transport. */
  navigationMode: "project" | "currentProject";
};

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
  /** App Shell-owned presentation and protocol identity. */
  shell: AppShellBootstrap;
  /** Host-issued identity unique to this webview instance. */
  clientInstanceId?: string;
  /** App Shell-owned editor focus used only to highlight Task Navigation. */
  focusedTaskId?: string | null;
  taskId?: string;
  agentId?: string;
  nativeSessionId?: string;
  projectId?: string;
  settingsTab?: SettingsTabId;
  /** Agent detail and return intent supplied by a recovery entry point. */
  settingsAgentId?: string;
  returnToNewTask?: boolean;
  preferences?: AppPreferencesRecord;
  appServerConnection?: WebviewAppServerConnection;
};
