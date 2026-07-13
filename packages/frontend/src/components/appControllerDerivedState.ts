import type { TaskSummary } from "@openaide/app-shell-contracts";
import type { AppState } from "../state/store";

export type AppControllerDerivedState = {
  activeTask?: TaskSummary;
  activeNavigationTaskId?: string;
  hasActiveTask: boolean;
  visibleTasks: AppState["tasks"];
};

export function appControllerDerivedStateDeps(state: AppState) {
  return [
    state.activeTaskId,
    state.searchQuery,
    state.tasks,
  ] as const;
}

export function deriveAppControllerState(state: AppState): AppControllerDerivedState {
  const activeTask = state.tasks.find((task) => task.task_id === state.activeTaskId);
  const filteredTasks = visibleTasks(state.tasks, state.searchQuery);
  const activeTaskShouldStayVisible = activeTask?.has_messages === true;
  return {
    activeTask,
    activeNavigationTaskId: activeTaskShouldStayVisible ? activeTask.task_id : undefined,
    hasActiveTask: activeTask !== undefined,
    visibleTasks: activeTaskShouldStayVisible && !filteredTasks.some((task) => task.task_id === activeTask.task_id)
      ? [...filteredTasks, activeTask]
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
