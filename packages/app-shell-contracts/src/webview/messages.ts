import type { RuntimeSettingsResult } from "../runtime/system.js";
import type {
  SecretTransactionMessage,
  SecretTransactionResultMessage,
} from "./secrets.js";
import type {
  DiagnosticsSnapshot,
  WorkspaceRootSummary,
} from "./settings.js";
import type { SettingsTabId } from "./preferences.js";
import type { WebviewAppServerConnection } from "./bootstrap.js";
import type { WebviewTelemetryPayload } from "./telemetry.js";

export type RuntimeErrorPayload = {
  message: string;
  action: WebviewToHostMessage["type"];
  task_id?: string;
  options_request_key?: string;
  session_list_request_id?: number;
  session_list_request_key?: string;
  artifact_id?: string;
  request_id?: string;
};

export type AppServerServerRequestMessage = {
  type: "appServer.serverRequest";
  payload: {
    requestId: string;
    method: string;
    params: unknown;
  };
};

export type AppServerServerRequestResultMessage = {
  type: "appServer.serverRequest.result";
  payload: {
    requestId: string;
    method: string;
    result: unknown;
  };
};

export type WebviewToHostMessage =
  | { type: "webview.telemetry"; payload: WebviewTelemetryPayload }
  | AppServerServerRequestMessage
  | SecretTransactionMessage
  | { type: "diagnostics.snapshot" }
  | { type: "diagnostics.export" }
  | { type: "workspace.roots" }
  | { type: "workspace.openFolder" }
  | { type: "developer.settings.unlock" }
  | { type: "surface.openNewTask"; payload?: { project_id?: string } }
  | { type: "surface.retainNewTaskProject"; payload: { project_id: string } }
  | { type: "surface.openNativeSession"; payload: { agent_id: string; native_session_id: string; project_id?: string } }
  | { type: "surface.openArchive" }
  | { type: "surface.openSettings"; payload?: { agent_id?: string; return_to_new_task?: boolean; project_id?: string; settings_tab?: SettingsTabId } }
  | { type: "surface.openTask"; payload: { task_id: string; title?: string; agent_id?: string } }
  | { type: "surface.updateTaskTitle"; payload: { task_id: string; title: string } }
  | { type: "shell.openExternal"; payload: { url: string } }
  | { type: "shell.reload" }
  | { type: "shell.clipboard.writeText"; payload: { requestId: string; text: string } }
  | { type: "supportExport.save"; payload: { requestId: string; fileHandleId: string; label: string; clientInstanceId: string } }
  | { type: "attachment.pickFiles"; payload: { requestId: string; taskId: string } }
  | { type: "project.pickFolder"; payload: { requestId: string } }
  | { type: "worktree.openFolder"; payload: { repository_id: string; worktree_id: string } }
  | { type: "tool.openPath"; payload: { path: string; line?: number } };

export type HostToWebviewMessage =
  | AppServerServerRequestResultMessage
  | SecretTransactionResultMessage
  | { type: "appServer.connectionChanged"; payload: { connection: WebviewAppServerConnection } }
  | { type: "surface.focusChanged"; payload: { task_id?: string } }
  | { type: "attachment.pickFiles.result"; payload: { requestId: string; attachments?: Array<{ handleId: string; label: string }>; error?: string } }
  | { type: "project.pickFolder.result"; payload: { requestId: string; folder?: { path: string; label: string }; error?: string } }
  | { type: "shell.clipboard.writeText.result"; payload: { requestId: string; ok: boolean; error?: string } }
  | { type: "supportExport.save.result"; payload: { requestId: string; ok: boolean; outcome?: "saved" | "cancelled"; error?: string } }
  | { type: "surface.workspaceChanged"; payload: { project_ids: string[] } }
  | { type: "surface.newTaskChanged"; payload: { project_id?: string } }
  | { type: "surface.routeChanged"; payload: { surface: "task"; task_id: string } }
  | { type: "surface.settingsChanged"; payload: { agent_id?: string; return_to_new_task?: boolean; project_id?: string; settings_tab?: SettingsTabId } }
  | { type: "diagnostics.snapshot.result"; payload: DiagnosticsSnapshot }
  | { type: "workspace.roots.result"; payload: { roots: WorkspaceRootSummary[] } }
  | { type: "runtime.settings.result"; payload: RuntimeSettingsResult }
  | { type: "runtime.error"; payload: RuntimeErrorPayload }
  | { type: "newTask" }
  | { type: "showSettings" };
