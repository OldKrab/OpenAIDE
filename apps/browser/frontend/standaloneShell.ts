import type { FrontendShell } from "../../../packages/frontend/src/services/frontendShell";
import { createStandaloneHost, standaloneBootstrap } from "./standaloneHost";
import { subscribeWindowMessages } from "../../../packages/frontend/src/shells/domBootstrap";

/** Standalone preview adapter used when no Web or VS Code shell bootstraps the page. */
export function createStandaloneShell(): FrontendShell {
  const host = createStandaloneHost();
  return {
    bootstrap: () => standaloneBootstrap() ?? { surface: "invalid" },
    messages: {
      post: (message) => host?.postMessage(message),
      subscribe: subscribeWindowMessages,
    },
    navigation: {
      openNewTask: (projectId) => host?.postMessage(projectId
        ? { type: "surface.openNewTask", payload: { project_id: projectId } }
        : { type: "surface.openNewTask" }),
      openNativeSession: (agentId, nativeSessionId, projectId) => host?.postMessage({
        type: "surface.openNativeSession",
        payload: {
          agent_id: agentId,
          native_session_id: nativeSessionId,
          ...(projectId ? { project_id: projectId } : {}),
        },
      }),
      openSettings: (agentId, returnToNewTask, projectId) => host?.postMessage({
        type: "surface.openSettings",
        payload: {
          ...(agentId ? { agent_id: agentId } : {}),
          ...(returnToNewTask ? { return_to_new_task: true } : {}),
          ...(projectId ? { project_id: projectId } : {}),
        },
      }),
      openTask: (taskId, title) => host?.postMessage({
        type: "surface.openTask",
        payload: { task_id: taskId, ...(title ? { title } : {}) },
      }),
      replaceSettingsTab: () => undefined,
      subscribe: () => () => undefined,
    },
    recovery: {
      openExternal: (url) => window.open(url, "_blank", "noopener,noreferrer"),
      reload: () => window.location.reload(),
    },
  };
}
