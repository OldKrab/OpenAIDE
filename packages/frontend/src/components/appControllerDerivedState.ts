import type { TaskSummary } from "@openaide/app-shell-contracts";
import type { AppState } from "../state/store";

export const PENDING_NEW_TASK_ID = "__pending_new_task__";

export type AppControllerDerivedState = {
  activeTask?: TaskSummary;
  activeNavigationTaskId?: string;
  hasActiveTask: boolean;
  visibleTasks: AppState["tasks"];
};

export function appControllerDerivedStateDeps(state: AppState) {
  return [
    state.activeTaskId,
    state.newTask.pending,
    state.newTask.selection,
    state.newTask.submitting,
    state.searchQuery,
    state.showArchived,
    state.taskInputs,
    state.tasks,
  ] as const;
}

export function deriveAppControllerState(state: AppState): AppControllerDerivedState {
  const activeTask = state.tasks.find((task) => task.task_id === state.activeTaskId);
  const tasksWithPendingPresentation = state.tasks.map((task) => taskWithPendingPresentation(task, state));
  const filteredTasks = visibleTasks(tasksWithPendingPresentation, state.searchQuery);
  const pendingNewTask = activeTask ? undefined : pendingNewTaskSummary(state);
  const visibleTasksWithPending = pendingNewTask
    ? [pendingNewTask, ...filteredTasks.filter((task) => task.task_id !== pendingNewTask.task_id)]
    : filteredTasks;
  const activeTaskShouldStayVisible = activeTask
    ? activeTask.has_messages || pendingTaskPresentation(activeTask, state) !== undefined
    : false;
  const activeVisibleTask = activeTask ? taskWithPendingPresentation(activeTask, state) : undefined;
  return {
    activeTask,
    activeNavigationTaskId: activeTask?.task_id ?? pendingNewTask?.task_id,
    hasActiveTask: activeTask !== undefined,
    visibleTasks: activeTaskShouldStayVisible && activeVisibleTask && !visibleTasksWithPending.some((task) => task.task_id === activeVisibleTask.task_id)
      ? [...visibleTasksWithPending, activeVisibleTask]
      : visibleTasksWithPending,
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

function pendingNewTaskSummary(state: AppState): TaskSummary | undefined {
  const pending = state.newTask.pending;
  const title = pending?.prompt.trim();
  if (state.showArchived || !state.newTask.submitting || !title) return undefined;
  return {
    agent_id: state.newTask.selection.agentId,
    agent_name: state.newTask.selection.agentLabel,
    created_at: "9999-12-31T23:59:59.999Z",
    has_messages: true,
    isolation: state.newTask.selection.isolation,
    last_activity: "9999-12-31T23:59:59.999Z",
    message_history_version: 0,
    project_id: state.newTask.selection.projectId,
    project_label: state.newTask.selection.workspaceLabel,
    status: "active",
    task_id: PENDING_NEW_TASK_ID,
    task_version: 0,
    title,
    unread: false,
    updated_at: "9999-12-31T23:59:59.999Z",
    workspace_root: state.newTask.selection.workspaceRoot,
  };
}

function taskWithPendingPresentation(task: TaskSummary, state: AppState): TaskSummary {
  const pending = pendingTaskPresentation(task, state);
  const title = pending?.prompt.trim();
  if (task.has_messages || !title) return task;
  return {
    ...task,
    has_messages: true,
    status: "active",
    title,
  };
}

function pendingTaskPresentation(task: TaskSummary, state: AppState) {
  const pendingInput = state.taskInputs[task.task_id]?.pending;
  if (pendingInput) return pendingInput;

  // Task creation selects the real empty task before its first-send input is installed.
  // Keep the submitted draft attached to that task across this lifecycle boundary.
  if (task.task_id === state.activeTaskId && state.newTask.submitting) {
    return state.newTask.pending;
  }
  return undefined;
}
