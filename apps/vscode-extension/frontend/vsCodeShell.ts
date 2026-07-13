import type { FrontendShell } from "../../../packages/frontend/src/services/frontendShell";
import type { WebviewBootstrap } from "../../../packages/frontend/src/state/surfaceTypes";
import {
  datasetBootstrap,
  subscribeWindowMessages,
} from "../../../packages/frontend/src/shells/domBootstrap";

declare global {
  interface Window {
    acquireVsCodeApi?: () => { postMessage: (message: unknown) => void };
  }
}

/** VS Code webview adapter; panel routing remains owned by the extension host. */
export function createVsCodeShell(): FrontendShell {
  const vscode = window.acquireVsCodeApi?.();
  const bootstrap = datasetBootstrap;
  return {
    bootstrap,
    messages: {
      post: (message) => vscode?.postMessage(message),
      subscribe: subscribeWindowMessages,
    },
    navigation: {
      openNewTask: (projectId) => vscode?.postMessage(projectId
        ? { type: "surface.openNewTask", payload: { project_id: projectId } }
        : { type: "surface.openNewTask" }),
      openSettings: () => vscode?.postMessage({ type: "surface.openSettings" }),
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
  };
}

function bootstrapForRouteMessage(message: unknown, current: WebviewBootstrap): WebviewBootstrap | undefined {
  if (!message || typeof message !== "object") return undefined;
  const candidate = message as { type?: unknown; payload?: { surface?: unknown; task_id?: unknown } };
  if (
    candidate.type !== "surface.routeChanged"
    || candidate.payload?.surface !== "task"
    || typeof candidate.payload.task_id !== "string"
    || !candidate.payload.task_id
  ) return undefined;
  return current.surface === "invalid"
    ? { surface: "task", taskId: candidate.payload.task_id }
    : {
        ...current,
        surface: "task",
        taskId: candidate.payload.task_id,
        projectId: undefined,
        settingsTab: undefined,
        archived: undefined,
      };
}
