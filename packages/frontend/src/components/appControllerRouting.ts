import { useEffect, useRef, useState, type Dispatch, type RefObject } from "react";
import type { BackendConnection } from "@openaide/app-server-client";
import { subscribeSurfaceRouteChanges } from "../services/hostBridge";
import { refreshSettingsProjectionsThroughBackend } from "../intents/settingsProjectionIntents";
import type { AppAction } from "../state/appReducer";
import type { AgentOption } from "../state/composerOptions";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import type { AppState } from "../state/store";
import {
  navigationTargetForBootstrap,
  type AsyncOperationOwner,
} from "../state/asyncOperationOwner";

export function useRoutedBootstrap(
  initialBootstrap: WebviewBootstrap,
  asyncOperations: AsyncOperationOwner,
  dispatch: Dispatch<AppAction>,
) {
  const [bootstrap, setBootstrap] = useState(initialBootstrap);
  const bootstrapRef = useRef(bootstrap);
  bootstrapRef.current = bootstrap;

  useEffect(() => {
    return subscribeSurfaceRouteChanges((nextBootstrap) => {
      asyncOperations.observeNavigation(
        navigationTargetForBootstrap(nextBootstrap),
        nextBootstrap.surface === "navigation" ? nextBootstrap.archived === true : undefined,
      );
      setBootstrap(nextBootstrap);
      if (nextBootstrap.surface !== "task") return;
      if (nextBootstrap.taskId) {
        dispatch({ type: "selection:set", taskId: nextBootstrap.taskId });
        return;
      }
      if (nextBootstrap.projectId) {
        dispatch({ type: "newTask:projectId", projectId: nextBootstrap.projectId });
      }
      // New Task navigation reopens the retained client-private instance. Only an
      // explicit discard or context replacement may reset its composer state.
      dispatch({ type: "selection:clear" });
    });
  }, [asyncOperations, dispatch]);

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
  compatibilityMessage?: string,
) {
  if (bootstrap.surface === "navigation") {
    dispatch({
      type: "tasks:error",
      message: compatibilityMessage ?? "Unable to load tasks from App Server",
    });
  }
  if (bootstrap.surface === "task" && bootstrap.taskId) {
    dispatch({
      type: "taskOpen:error",
      taskId: bootstrap.taskId,
      message: compatibilityMessage ?? "Unable to open task from App Server",
    });
  }
}
