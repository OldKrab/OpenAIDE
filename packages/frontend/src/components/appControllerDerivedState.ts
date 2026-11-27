import type { TaskSummary } from "@openaide/app-shell-contracts";
import type { AppState } from "../state/store";

export type AppControllerDerivedState = {
  activeTask?: TaskSummary;
  activeNavigationTaskId?: string;
  hasActiveTask: boolean;
  /** The Tasks of the Navigation view; search is App Server's, in its own result list. */
  visibleTasks: AppState["tasks"];
};

export function appControllerDerivedStateDeps(
  state: AppState,
  navigationFocusedTaskId?: string | null,
) {
  return [
    state.activeTaskId,
    state.tasks,
    state.snapshot,
    navigationFocusedTaskId,
  ] as const;
}

export function deriveAppControllerState(
  state: AppState,
  navigationFocusedTaskId?: string | null,
): AppControllerDerivedState {
  // The open Task's summary comes from its own Task subscription, not from a Navigation row:
  // a Task outside the view's window (opened by link or search) is still the active Task.
  const activeTask = state.snapshot?.task.task_id === state.activeTaskId
    ? state.snapshot?.task
    : state.tasks.find((task) => task.task_id === state.activeTaskId);
  const navigationTask = navigationFocusedTaskId === undefined
    ? activeTask
    : navigationFocusedTaskId === null
      ? undefined
      : state.tasks.find((task) => task.task_id === navigationFocusedTaskId);
  const navigationTaskShouldStayVisible = navigationTask?.has_messages === true;
  return {
    activeTask,
    activeNavigationTaskId: navigationTaskShouldStayVisible ? navigationTask.task_id : undefined,
    hasActiveTask: activeTask !== undefined,
    visibleTasks: state.tasks,
  };
}
