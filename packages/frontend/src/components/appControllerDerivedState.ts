import type { TaskSummary } from "@openaide/app-shell-contracts";
import type { AppState } from "../state/store";

export type AppControllerDerivedState = {
  activeTask?: TaskSummary;
  activeNavigationTaskId?: string;
  hasActiveTask: boolean;
  visibleTasks: AppState["tasks"];
};

export function appControllerDerivedStateDeps(
  state: AppState,
  navigationFocusedTaskId?: string | null,
) {
  return [
    state.activeTaskId,
    state.searchQuery,
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
    visibleTasks: visibleTasks(state.tasks, state.searchQuery),
  };
}

export function visibleTasks(tasks: AppState["tasks"], searchQuery: string): AppState["tasks"] {
  // TODO(state-sync phase 7): Search is server-side; until then it filters the rows the view loaded.
  const query = searchQuery.trim().toLowerCase();
  if (!query) return tasks;
  return tasks.filter((task) =>
    [task.title, task.agent_name, task.status].some((value) => value.toLowerCase().includes(query)),
  );
}
