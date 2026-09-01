import type { FrontendShell } from "../../../packages/frontend/src/services/frontendShell";
import {
  ATTACHMENT_REVEAL_SENT,
  createBridgedAppServerSession,
  isAppServerSessionHostMessage,
  type TaskId,
} from "@openaide/app-server-client";
import type { WebviewBootstrap } from "../../../packages/frontend/src/state/surfaceTypes";
import {
  datasetBootstrap,
  subscribeWindowMessages,
} from "../../../packages/frontend/src/shells/domBootstrap";
import { VSCODE_SHELL } from "../src/webview/types";
import type { HostToWebviewMessage } from "@openaide/app-shell-contracts";
import type { PreSendAttachment } from "@openaide/app-server-client";
import { clientInstanceIdForBootstrap } from "../../../packages/frontend/src/services/backendInitialization";

declare global {
  interface Window {
    acquireVsCodeApi?: () => { postMessage: (message: unknown) => void };
  }
}

/** VS Code webview adapter; panel routing remains owned by the extension host. */
export function createVsCodeShell(): FrontendShell {
  const vscode = window.acquireVsCodeApi?.();
  const initialBootstrap = readDatasetBootstrap();
  let currentBootstrap = initialBootstrap;
  let nextFileRequest = 1;
  let nextProjectRequest = 1;
  let nextClipboardRequest = 1;
  let nextSupportExportRequest = 1;
  const pendingSupportExportRequests = new Map<string, {
    resolve: () => void;
    reject: (error: Error) => void;
  }>();
  const pendingClipboardRequests = new Map<string, {
    resolve: () => void;
    reject: (error: Error) => void;
  }>();
  const pendingFileRequests = new Map<string, {
    resolve: (attachments: PreSendAttachment[]) => void;
    reject: (error: Error) => void;
  }>();
  const pendingProjectRequests = new Map<string, {
    resolve: (folder: { path: string; label: string } | undefined) => void;
    reject: (error: Error) => void;
  }>();
  if (typeof window.addEventListener === "function") {
    window.addEventListener("message", (event: MessageEvent<HostToWebviewMessage>) => {
      if (event.data?.type !== "supportExport.save.result") return;
      const pending = pendingSupportExportRequests.get(event.data.payload.requestId);
      if (!pending) return;
      pendingSupportExportRequests.delete(event.data.payload.requestId);
      if (!event.data.payload.ok) pending.reject(new Error(event.data.payload.error ?? "Unable to save support export."));
      else pending.resolve();
    });
    window.addEventListener("message", (event: MessageEvent<HostToWebviewMessage>) => {
      if (event.data?.type !== "shell.clipboard.writeText.result") return;
      const pending = pendingClipboardRequests.get(event.data.payload.requestId);
      if (!pending) return;
      pendingClipboardRequests.delete(event.data.payload.requestId);
      if (!event.data.payload.ok) pending.reject(new Error(event.data.payload.error ?? "Unable to copy text."));
      else pending.resolve();
    });
    window.addEventListener("message", (event: MessageEvent<HostToWebviewMessage>) => {
      if (event.data?.type !== "attachment.pickFiles.result") return;
      const pending = pendingFileRequests.get(event.data.payload.requestId);
      if (!pending) return;
      pendingFileRequests.delete(event.data.payload.requestId);
      if (event.data.payload.error) pending.reject(new Error(event.data.payload.error));
      else pending.resolve((event.data.payload.attachments ?? []) as PreSendAttachment[]);
    });
    window.addEventListener("message", (event: MessageEvent<HostToWebviewMessage>) => {
      if (event.data?.type !== "project.pickFolder.result") return;
      const pending = pendingProjectRequests.get(event.data.payload.requestId);
      if (!pending) return;
      pendingProjectRequests.delete(event.data.payload.requestId);
      if (event.data.payload.error) pending.reject(new Error(event.data.payload.error));
      else pending.resolve(event.data.payload.folder);
    });
  }
  const backendConnection = vscode && typeof window.addEventListener === "function"
    ? createBridgedAppServerSession({
        post: (message) => vscode.postMessage(message),
        subscribe(listener) {
          const onMessage = (event: MessageEvent) => {
            if (isAppServerSessionHostMessage(event.data)) listener(event.data);
          };
          window.addEventListener("message", onMessage);
          return () => window.removeEventListener("message", onMessage);
        },
      })
    : undefined;
  return {
    bootstrap: () => bootstrapWithCurrentDocument(currentBootstrap),
    ...(backendConnection ? { backendConnection: () => backendConnection } : {}),
    clipboard: {
      writeText(text) {
        if (!vscode) return Promise.reject(new Error("VS Code clipboard unavailable."));
        const requestId = `clipboard-write-${nextClipboardRequest++}`;
        return new Promise((resolve, reject) => {
          pendingClipboardRequests.set(requestId, { resolve, reject });
          vscode.postMessage({ type: "shell.clipboard.writeText", payload: { requestId, text } });
        });
      },
    },
    sentFiles: {
      sentFileAction: "reveal",
      openSentFile({ attachmentIndex, messageId, taskId }) {
        void backendConnection?.request(ATTACHMENT_REVEAL_SENT, {
          taskId: taskId as TaskId,
          messageId,
          attachmentIndex,
        });
      },
    },
    supportExports: {
      save({ fileHandleId, label }) {
        if (!vscode) return Promise.reject(new Error("VS Code save dialog unavailable."));
        const requestId = `support-export-save-${nextSupportExportRequest++}`;
        return new Promise((resolve, reject) => {
          pendingSupportExportRequests.set(requestId, { resolve, reject });
          vscode.postMessage({
            type: "supportExport.save",
            payload: {
              requestId,
              fileHandleId,
              label,
              clientInstanceId: clientInstanceIdForBootstrap(initialBootstrap),
            },
          });
        });
      },
    },
    files: {
      kind: "nativePicker",
      pick(taskId) {
        if (!vscode) return Promise.reject(new Error("VS Code file picker unavailable."));
        const requestId = `attachment-pick-${nextFileRequest++}`;
        return new Promise((resolve, reject) => {
          pendingFileRequests.set(requestId, { resolve, reject });
          vscode.postMessage({ type: "attachment.pickFiles", payload: { requestId, taskId } });
        });
      },
    },
    messages: {
      post: (message) => vscode?.postMessage(message),
      subscribe: subscribeWindowMessages,
    },
    navigation: {
      openNewTask: (projectId) => vscode?.postMessage(projectId
        ? { type: "surface.openNewTask", payload: { project_id: projectId } }
        : { type: "surface.openNewTask" }),
      retainNewTaskProject: (projectId) => vscode?.postMessage({
        type: "surface.retainNewTaskProject",
        payload: { project_id: projectId },
      }),
      openNativeSession: (agentId, nativeSessionId, projectId) => vscode?.postMessage({
        type: "surface.openNativeSession",
        payload: {
          agent_id: agentId,
          native_session_id: nativeSessionId,
          ...(projectId ? { project_id: projectId } : {}),
        },
      }),
      openSettings: (agentId, returnToNewTask, projectId, settingsTab) => vscode?.postMessage({
        type: "surface.openSettings",
        payload: {
          ...(agentId ? { agent_id: agentId } : {}),
          ...(returnToNewTask ? { return_to_new_task: true } : {}),
          ...(projectId ? { project_id: projectId } : {}),
          ...(settingsTab ? { settings_tab: settingsTab } : {}),
        },
      }),
      openTask: (taskId, title, agentId) => vscode?.postMessage({
        type: "surface.openTask",
        payload: {
          task_id: taskId,
          ...(title ? { title } : {}),
          ...(agentId ? { agent_id: agentId } : {}),
        },
      }),
      replaceSettingsTab: () => undefined,
      subscribe(listener) {
        const onMessage = (event: MessageEvent) => {
          const next = bootstrapForRouteMessage(event.data, currentBootstrap);
          if (!next) return;
          currentBootstrap = next;
          listener(next);
        };
        window.addEventListener("message", onMessage);
        return () => window.removeEventListener("message", onMessage);
      },
    },
    recovery: {
      openExternal: (url) => vscode?.postMessage({ type: "shell.openExternal", payload: { url } }),
      reload: () => vscode?.postMessage({ type: "shell.reload" }),
    },
    workspace: {
      openFolder: () => vscode?.postMessage({ type: "workspace.openFolder" }),
    },
    projects: {
      pickFolder() {
        if (!vscode) return Promise.reject(new Error("VS Code folder picker unavailable."));
        const requestId = `project-pick-${nextProjectRequest++}`;
        return new Promise((resolve, reject) => {
          pendingProjectRequests.set(requestId, { resolve, reject });
          vscode.postMessage({ type: "project.pickFolder", payload: { requestId } });
        });
      },
    },
  };
}

function bootstrapForRouteMessage(message: unknown, current: WebviewBootstrap): WebviewBootstrap | undefined {
  if (!message || typeof message !== "object") return undefined;
  const candidate = message as { type?: unknown; payload?: { surface?: unknown; task_id?: unknown; agent_id?: unknown; return_to_new_task?: unknown; project_id?: unknown; project_ids?: unknown; settings_tab?: unknown } };
  if (candidate.type === "surface.workspaceChanged") {
    const projectIds = candidate.payload?.project_ids;
    if (!Array.isArray(projectIds) || projectIds.some((projectId) => typeof projectId !== "string")) return undefined;
    return current.surface === "invalid" ? undefined : { ...current, projectIds };
  }
  if (candidate.type === "surface.settingsChanged") {
    return current.surface === "invalid" ? undefined : {
      ...current,
      surface: "settings",
      settingsAgentId: typeof candidate.payload?.agent_id === "string" ? candidate.payload.agent_id : undefined,
      settingsTab: isSettingsTab(candidate.payload?.settings_tab) ? candidate.payload.settings_tab : undefined,
      returnToNewTask: candidate.payload?.return_to_new_task === true,
      projectId: typeof candidate.payload?.project_id === "string" ? candidate.payload.project_id : undefined,
      taskId: undefined,
    };
  }
  if (candidate.type === "surface.newTaskChanged") {
    if (candidate.payload?.project_id !== undefined && typeof candidate.payload.project_id !== "string") {
      return undefined;
    }
    return current.surface === "invalid" ? undefined : {
      ...current,
      surface: "task",
      taskId: undefined,
      projectId: typeof candidate.payload?.project_id === "string" ? candidate.payload.project_id : undefined,
      settingsTab: undefined,
      archived: undefined,
    };
  }
  if (
    candidate.type !== "surface.routeChanged"
    || candidate.payload?.surface !== "task"
    || typeof candidate.payload.task_id !== "string"
    || !candidate.payload.task_id
  ) return undefined;
  return current.surface === "invalid"
    ? { surface: "task", shell: VSCODE_SHELL, taskId: candidate.payload.task_id }
    : {
        ...current,
        surface: "task",
        taskId: candidate.payload.task_id,
        projectId: undefined,
        settingsTab: undefined,
        archived: undefined,
      };
}

function bootstrapWithCurrentDocument(current: WebviewBootstrap): WebviewBootstrap {
  const latest = readDatasetBootstrap();
  if (current.surface === "invalid" || latest.surface === "invalid") {
    return current.surface === "invalid" ? latest : current;
  }
  return {
    ...current,
    clientInstanceId: latest.clientInstanceId,
    shell: latest.shell ?? current.shell,
    appServerConnection: latest.appServerConnection,
    preferences: latest.preferences,
  };
}

function readDatasetBootstrap(): WebviewBootstrap {
  return typeof document === "undefined" ? { surface: "invalid" } : datasetBootstrap();
}

function isSettingsTab(value: unknown): value is import("@openaide/app-shell-contracts").SettingsTabId {
  return value === "agents"
    || value === "mcp"
    || value === "skills"
    || value === "common"
    || value === "desktop"
    || value === "data"
    || value === "worktrees";
}
