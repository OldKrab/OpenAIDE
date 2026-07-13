import type { FrontendShell } from "../services/frontendShell";
import { createStandaloneHost, standaloneBootstrap } from "../services/devHost";
import { subscribeWindowMessages } from "./domBootstrap";

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
      openSettings: () => host?.postMessage({ type: "surface.openSettings" }),
      openTask: (taskId, title) => host?.postMessage({
        type: "surface.openTask",
        payload: { task_id: taskId, ...(title ? { title } : {}) },
      }),
      replaceSettingsTab: () => undefined,
      subscribe: () => () => undefined,
    },
  };
}
