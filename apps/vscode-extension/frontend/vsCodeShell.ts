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

declare global {
  interface Window {
    acquireVsCodeApi?: () => { postMessage: (message: unknown) => void };
  }
}

/** VS Code webview adapter; panel routing remains owned by the extension host. */
export function createVsCodeShell(): FrontendShell {
  const vscode = window.acquireVsCodeApi?.();
  const bootstrap = datasetBootstrap;
  let nextFileRequest = 1;
  const pendingFileRequests = new Map<string, {
    resolve: (attachments: PreSendAttachment[]) => void;
    reject: (error: Error) => void;
  }>();
  if (typeof window.addEventListener === "function") {
    window.addEventListener("message", (event: MessageEvent<HostToWebviewMessage>) => {
      if (event.data?.type !== "attachment.pickFiles.result") return;
      const pending = pendingFileRequests.get(event.data.payload.requestId);
      if (!pending) return;
      pendingFileRequests.delete(event.data.payload.requestId);
      if (event.data.payload.error) pending.reject(new Error(event.data.payload.error));
      else pending.resolve((event.data.payload.attachments ?? []) as PreSendAttachment[]);
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
    bootstrap,
    ...(backendConnection ? { backendConnection: () => backendConnection } : {}),
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
      openSettings: (agentId, returnToNewTask, projectId) => vscode?.postMessage({
        type: "surface.openSettings",
        payload: {
          ...(agentId ? { agent_id: agentId } : {}),
          ...(returnToNewTask ? { return_to_new_task: true } : {}),
          ...(projectId ? { project_id: projectId } : {}),
        },
      }),
      openTask: (taskId, title) => vscode?.postMessage({
        type: "surface.openTask",
        payload: { task_id: taskId, ...(title ? { title } : {}) },
      }),
      replaceSettingsTab: () => undefined,
      subscribe(listener) {
        const onMessage = (event: MessageEvent) => {
          const next = bootstrapForRouteMessage(event.data, bootstrap());
          if (next) listener(next);
        };
        window.addEventListener("message", onMessage);
        return () => window.removeEventListener("message", onMessage);
      },
    },
    recovery: {
      openExternal: (url) => vscode?.postMessage({ type: "shell.openExternal", payload: { url } }),
      reload: () => vscode?.postMessage({ type: "shell.reload" }),
    },
  };
}

function bootstrapForRouteMessage(message: unknown, current: WebviewBootstrap): WebviewBootstrap | undefined {
  if (!message || typeof message !== "object") return undefined;
  const candidate = message as { type?: unknown; payload?: { surface?: unknown; task_id?: unknown; agent_id?: unknown; return_to_new_task?: unknown; project_id?: unknown } };
  if (candidate.type === "surface.settingsChanged") {
    return current.surface === "invalid" ? undefined : {
      ...current,
      surface: "settings",
      settingsAgentId: typeof candidate.payload?.agent_id === "string" ? candidate.payload.agent_id : undefined,
      returnToNewTask: candidate.payload?.return_to_new_task === true,
      projectId: typeof candidate.payload?.project_id === "string" ? candidate.payload.project_id : undefined,
      taskId: undefined,
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
