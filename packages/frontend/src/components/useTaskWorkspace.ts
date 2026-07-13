import { useEffect, useMemo, type Dispatch } from "react";
import type { BackendConnection } from "@openaide/app-server-client";
import type { AppAction } from "../state/appReducer";
import type { AppState } from "../state/store";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import { openNewTaskSurface, postHostMessage } from "../services/hostBridge";
import { sendWebviewTelemetry } from "../state/hostMessageRouter";
import { appControllerDerivedStateDeps, deriveAppControllerState } from "./appControllerDerivedState";
import { useTaskAttentionReadReceipt } from "./useTaskAttentionReadReceipt";

type TaskWorkspaceOptions = {
  backendConnection?: Pick<BackendConnection, "request">;
  bootstrap: WebviewBootstrap;
  dispatch: Dispatch<AppAction>;
  state: AppState;
};

/** Projects visible Task workspace state and owns route-level Task presentation effects. */
export function useTaskWorkspace({ backendConnection, bootstrap, dispatch, state }: TaskWorkspaceOptions) {
  useTaskAttentionReadReceipt({
    backendConnection,
    dispatch,
    revision: state.snapshot?.revision,
    taskId: bootstrap.surface === "task" ? state.snapshot?.task.task_id : undefined,
    unread: state.snapshot?.task.unread === true,
  });

  useEffect(() => {
    const snapshotTaskId = state.snapshot?.task.task_id;
    const snapshotHasPendingInput = snapshotTaskId
      ? state.taskInputs[snapshotTaskId]?.pending !== undefined
      : false;
    if (
      bootstrap.surface !== "task"
      || !bootstrap.taskId
      || !state.snapshot
      || state.snapshot.task.has_messages
      || state.snapshot.task.status !== "inactive"
      || snapshotHasPendingInput
    ) return;
    openNewTaskSurface(state.snapshot.task.project_id);
  }, [
    bootstrap.surface,
    bootstrap.taskId,
    state.snapshot?.task.task_id,
    state.snapshot?.task.has_messages,
    state.snapshot?.task.status,
    state.snapshot?.task.project_id,
    state.snapshot ? state.taskInputs[state.snapshot.task.task_id]?.pending : undefined,
  ]);

  const derivedStateDeps = appControllerDerivedStateDeps(state);
  const derived = useMemo(() => deriveAppControllerState(state), derivedStateDeps);

  useEffect(() => {
    if (!state.snapshot) return;
    sendWebviewTelemetry(postHostMessage, "task_rendered", {
      surface: bootstrap.surface,
      task_id: state.snapshot.task.task_id,
      task_status: state.snapshot.task.status,
      chat_items: state.snapshot.chat.items.length,
      has_active_task: derived.hasActiveTask,
    });
  }, [
    derived.hasActiveTask,
    bootstrap.surface,
    state.snapshot?.task.task_id,
    state.snapshot?.task.status,
    state.snapshot?.chat.items.length,
  ]);

  return derived;
}
