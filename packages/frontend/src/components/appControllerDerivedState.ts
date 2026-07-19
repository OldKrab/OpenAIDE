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
    navigationFocusedTaskId,
  ] as const;
}

export function deriveAppControllerState(
  state: AppState,
  navigationFocusedTaskId?: string | null,
): AppControllerDerivedState {
  const activeTask = state.tasks.find((task) => task.task_id === state.activeTaskId);
  const filteredTasks = visibleTasks(state.tasks, state.searchQuery);
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
    visibleTasks: navigationTaskShouldStayVisible && !filteredTasks.some((task) => task.task_id === navigationTask.task_id)
      ? [...filteredTasks, navigationTask]
      : filteredTasks,
  };
}

export function visibleTasks(tasks: AppState["tasks"], searchQuery: string): AppState["tasks"] {
  const query = searchQuery.trim().toLowerCase();
  const nonEmptyTasks = tasks.filter((task) => task.has_messages);
  if (!query) return nonEmptyTasks;
  return nonEmptyTasks.filter((task) =>
    [task.title, task.agent_name, task.status].some((value) => value.toLowerCase().includes(query)),
  );
}
