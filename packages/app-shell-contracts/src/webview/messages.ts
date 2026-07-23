import type { RuntimeSettingsResult } from "../runtime/system.js";
import type {
  SecretTransactionMessage,
  SecretTransactionResultMessage,
} from "./secrets.js";
import type {
  DiagnosticsSnapshot,
  WorkspaceRootSummary,
} from "./settings.js";
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
  | { type: "developer.settings.unlock" }
  | { type: "surface.openNewTask"; payload?: { project_id?: string } }
  | { type: "surface.openNativeSession"; payload: { agent_id: string; native_session_id: string; project_id?: string } }
  | { type: "surface.openArchive" }
  | { type: "surface.openSettings"; payload?: { agent_id?: string; return_to_new_task?: boolean; project_id?: string } }
  | { type: "surface.openTask"; payload: { task_id: string; title?: string } }
  | { type: "surface.updateTaskTitle"; payload: { task_id: string; title: string } }
  | { type: "shell.openExternal"; payload: { url: string } }
  | { type: "shell.reload" }
  | { type: "attachment.pickFiles"; payload: { requestId: string; taskId: string } }
  | { type: "worktree.openFolder"; payload: { repository_id: string; worktree_id: string } }
  | { type: "tool.openPath"; payload: { path: string; line?: number } };

export type HostToWebviewMessage =
  | AppServerServerRequestResultMessage
  | SecretTransactionResultMessage
  | { type: "appServer.connectionChanged"; payload: { connection: WebviewAppServerConnection } }
  | { type: "surface.focusChanged"; payload: { task_id?: string } }
  | { type: "attachment.pickFiles.result"; payload: { requestId: string; attachments?: Array<{ handleId: string; label: string }>; error?: string } }
  | { type: "surface.routeChanged"; payload: { surface: "task"; task_id: string } }
  | { type: "surface.settingsChanged"; payload: { agent_id?: string; return_to_new_task?: boolean; project_id?: string } }
  | { type: "diagnostics.snapshot.result"; payload: DiagnosticsSnapshot }
  | { type: "workspace.roots.result"; payload: { roots: WorkspaceRootSummary[] } }
  | { type: "runtime.settings.result"; payload: RuntimeSettingsResult }
  | { type: "runtime.error"; payload: RuntimeErrorPayload }
  | { type: "newTask" }
  | { type: "showSettings" };
