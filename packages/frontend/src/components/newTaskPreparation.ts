import {
  TASK_ACQUIRE,
  TASK_ACQUIRE_IN_WORKTREE,
  TASK_RELEASE,
  TASK_OPEN,
  type BackendConnection,
  type TaskId,
  type TaskSnapshot as ProtocolTaskSnapshot,
} from "@openaide/app-server-client";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import type { AppAction } from "../state/appReducer";
import {
  preparedTaskMatchesNewTaskContext,
  taskAcquireInWorktreeParams,
  taskAcquireParams,
} from "../state/newTaskPreparationContext";
import type { AppState } from "../state/store";

type PrepareNewTaskOptions = {
  acceptPreparedTask?: (task: ProtocolTaskSnapshot) => boolean;
  discardPreparedTask?: (taskId: TaskId) => Promise<void>;
  preparedTask?: ProtocolTaskSnapshot;
  reuseSnapshot?: boolean;
  snapshotIntent?: "open" | "refresh";
};

type PrepareNewTaskDependencies = {
  backendConnection?: Partial<Pick<BackendConnection, "request">>;
  dispatch: (action: AppAction) => void;
  onPreparedTask?: (task: ProtocolTaskSnapshot, taskId: TaskId) => void;
  state: AppState;
};

/** Acquires one empty Task that belongs to the current immutable preparation context. */
export async function prepareNewTask(
  { backendConnection, dispatch, onPreparedTask, state }: PrepareNewTaskDependencies,
  options: PrepareNewTaskOptions = {},
) {
  const request = backendConnection?.request;
  if (!request) throw new Error("App Server connection unavailable.");
  const projectId = state.newTask.selection.projectId;
  if (!projectId) throw new Error("Workspace unavailable. Refresh and try again.");

  const discardPreparedTask = async (taskId: TaskId) => {
    if (options.discardPreparedTask) {
      await options.discardPreparedTask(taskId);
      return;
    }
    try {
      await request(TASK_RELEASE, { taskId });
    } catch {
      // The replacement Task must not keep rendering the stale local Draft.
    }
    dispatch({ type: "taskInput:clear", taskId });
    dispatch({ type: "task:list:remove", taskId });
  };
  let preparedTask = options.preparedTask;
  if (preparedTask && !preparedProtocolTaskMatchesSelection(preparedTask, state)) {
    await discardPreparedTask(preparedTask.task.taskId as TaskId);
    preparedTask = undefined;
  }
  if (!preparedTask && options.reuseSnapshot !== false && state.snapshot && !state.snapshot.task.has_messages) {
    const staleOrReusableTaskId = state.snapshot.task.task_id as TaskId;
    if (preparedSnapshotMatchesSelection(state)) {
      const openedTask = await request(TASK_OPEN, { taskId: staleOrReusableTaskId }).then((result) => result.task);
      if (!openedTask || !preparedProtocolTaskMatchesSelection(openedTask, state)) {
        await discardPreparedTask(staleOrReusableTaskId);
      } else {
        preparedTask = openedTask;
      }
    } else {
      await discardPreparedTask(staleOrReusableTaskId);
    }
  }
  preparedTask ??= state.newTask.selection.worktreeId
    ? (await request(TASK_ACQUIRE_IN_WORKTREE, taskAcquireInWorktreeParams(state, projectId))).task
    : (await request(TASK_ACQUIRE, taskAcquireParams(state, projectId))).task;
  if (!preparedProtocolTaskMatchesSelection(preparedTask, state)) {
    await discardPreparedTask(preparedTask.task.taskId as TaskId);
    throw new Error("Prepared Task does not match the current New Task context.");
  }
  const taskId = preparedTask.task.taskId as TaskId;
  if (options.acceptPreparedTask && !options.acceptPreparedTask(preparedTask)) {
    return { task: preparedTask, taskId };
  }
  dispatch({
    type: "snapshot",
    snapshot: mapProtocolTaskSnapshot(preparedTask).snapshot,
    intent: options.snapshotIntent ?? "open",
  });
  dispatch({ type: "newTask:prepared", taskId });
  onPreparedTask?.(preparedTask, taskId);
  return { task: preparedTask, taskId };
}

export function preparedSnapshotMatchesSelection(state: AppState) {
  const task = state.snapshot?.task;
  return Boolean(
    task
    && !task.has_messages
    && preparedTaskMatchesNewTaskContext(state, {
      agentId: task.agent_id,
      projectId: task.project_id,
      workspaceRoot: task.workspace_root,
      worktreeId: task.worktree_id,
    }),
  );
}

export function preparedProtocolTaskMatchesSelection(
  task: ProtocolTaskSnapshot,
  state: AppState,
) {
  return preparedTaskMatchesNewTaskContext(state, {
    agentId: task.task.agentId,
    projectId: task.task.projectId,
    worktreeId: task.task.worktreeId ?? undefined,
  });
}
