import { useEffect, useRef, useState, type Dispatch, type RefObject } from "react";
import type { BackendConnection } from "@openaide/app-server-client";
import { subscribeSurfaceRouteChanges } from "../services/hostBridge";
import { refreshSettingsProjectionsThroughBackend } from "../intents/settingsProjectionIntents";
import type { AppAction } from "../state/appReducer";
import type { AgentOption } from "../state/composerOptions";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import type { AppState } from "../state/store";

export function useRoutedBootstrap(
  initialBootstrap: WebviewBootstrap,
  dispatch: Dispatch<AppAction>,
) {
  const [bootstrap, setBootstrap] = useState(initialBootstrap);
  const bootstrapRef = useRef(bootstrap);
  bootstrapRef.current = bootstrap;

  useEffect(() => {
    return subscribeSurfaceRouteChanges((nextBootstrap) => {
      setBootstrap(nextBootstrap);
      if (nextBootstrap.surface !== "task") return;
      if (nextBootstrap.taskId) {
        dispatch({ type: "selection:set", taskId: nextBootstrap.taskId });
        return;
      }
      if (nextBootstrap.projectId) {
        dispatch({ type: "newTask:projectId", projectId: nextBootstrap.projectId });
      }
      // A new-task route is a fresh composer even when a previously created Task is
      // still sending in the background through its task-local pending input.
      dispatch({ type: "newTask:reset" });
      dispatch({ type: "selection:clear" });
    });
  }, [dispatch]);

  return { bootstrap, bootstrapRef };
}

export function useSettingsRouteRefresh({
  backendConnectionRef,
  backendInitialized,
  bootstrap,
  currentAgentId,
  dispatch,
  setAgents,
  state,
}: {
  backendConnectionRef?: Pick<BackendConnection, "request">;
  backendInitialized: RefObject<boolean>;
  bootstrap: WebviewBootstrap;
  currentAgentId: RefObject<string>;
  dispatch: Dispatch<AppAction>;
  setAgents: (agents: AgentOption[]) => void;
  state: AppState;
}) {
  const stateRef = useRef(state);
  stateRef.current = state;

  useEffect(() => {
    if (bootstrap.surface !== "settings" || !backendInitialized.current || !backendConnectionRef?.request) return;
    dispatch({ type: "settings:start" });
    void refreshSettingsProjectionsThroughBackend({
      backendConnection: { request: backendConnectionRef.request },
      currentAgentId: currentAgentId.current,
      dispatch,
      setAgents,
      state: stateRef.current,
    }).catch((error) => {
      dispatch({
        type: "settings:error",
        message: error instanceof Error ? error.message : "Unable to load Agent settings from App Server",
      });
    });
  }, [backendConnectionRef, bootstrap.surface, backendInitialized, currentAgentId, dispatch, setAgents]);
}

export function dispatchStartupReadError(
  bootstrap: WebviewBootstrap,
  dispatch: Dispatch<AppAction>,
) {
  if (bootstrap.surface === "navigation") {
    dispatch({ type: "tasks:error", message: "Unable to load tasks from App Server" });
  }
  if (bootstrap.surface === "task" && bootstrap.taskId) {
    dispatch({
      type: "taskOpen:error",
      taskId: bootstrap.taskId,
      message: "Unable to open task from App Server",
    });
  }
}
