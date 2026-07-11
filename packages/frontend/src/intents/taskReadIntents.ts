import {
  TASK_LIST,
  TASK_OPEN,
  TASK_SET_ARCHIVED,
  type BackendConnection,
  type TaskId,
  type TaskListResult,
  type TaskOpenResult,
} from "@openaide/app-server-client";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import type { AppAction, SnapshotIntent } from "../state/appReducer";
import { mapProtocolTaskSnapshot, mapProtocolTaskSummary } from "../state/appServerProtocolMapping";

type TaskReadConnection = Pick<BackendConnection, "request">;

export type TaskReadIntentDependencies = {
  acceptTaskOpen?: (taskId: string, requestId: number | undefined, intent: SnapshotIntent) => boolean;
  acceptTaskList?: () => boolean;
  backendConnection: TaskReadConnection;
  createTaskOpenRequestId?: (taskId: string, intent: SnapshotIntent) => number;
  dispatch: (action: AppAction) => void;
};

export function requestMissingInitialTaskRead(
  dependencies: TaskReadIntentDependencies,
  bootstrap: WebviewBootstrap,
  snapshot: { tasks?: unknown; activeTask?: unknown },
) {
  if (bootstrap.surface === "navigation" && !snapshot.tasks) {
    void requestTaskList(dependencies, bootstrap.archived === true).catch((error) => {
      dependencies.dispatch({
        type: "tasks:error",
        message: error instanceof Error ? error.message : "Unable to load tasks from App Server",
      });
    });
  }

  if (bootstrap.surface === "task" && bootstrap.taskId && !snapshot.activeTask) {
    const taskId = bootstrap.taskId;
    void requestTaskOpen(dependencies, taskId, "open").catch(() => {
      dependencies.dispatch({
        type: "taskOpen:error",
        taskId,
        message: "Unable to open task from App Server",
      });
    });
  }
}

export async function requestTaskList({
  acceptTaskList,
  backendConnection,
  dispatch,
}: TaskReadIntentDependencies, archived = false) {
  const result = await backendConnection.request(TASK_LIST, { archived });
  if (acceptTaskList && !acceptTaskList()) return;
  dispatchTaskListResult(dispatch, result);
}

export async function requestTaskSetArchived(
  { backendConnection }: TaskReadIntentDependencies,
  taskId: string,
  archived: boolean,
) {
  await backendConnection.request(TASK_SET_ARCHIVED, {
    taskId: taskId as TaskId,
    archived,
  });
}

export async function requestTaskOpen(
  { acceptTaskOpen, backendConnection, createTaskOpenRequestId, dispatch }: TaskReadIntentDependencies,
  taskId: string,
  intent: SnapshotIntent = "open",
) {
  const requestId = createTaskOpenRequestId?.(taskId, intent);
  const result = await backendConnection.request(TASK_OPEN, { taskId: taskId as TaskId });
  dispatchTaskOpenResult(dispatch, result, intent, requestId, acceptTaskOpen);
}

function dispatchTaskListResult(dispatch: (action: AppAction) => void, result: TaskListResult) {
  dispatch({
    type: "tasks",
    tasks: result.tasks.map((task) => mapProtocolTaskSummary(task, result.revision)),
  });
}

function dispatchTaskOpenResult(
  dispatch: (action: AppAction) => void,
  result: TaskOpenResult,
  intent: SnapshotIntent,
  requestId: number | undefined,
  acceptTaskOpen: TaskReadIntentDependencies["acceptTaskOpen"],
) {
  const mapped = mapProtocolTaskSnapshot(result.task).snapshot;
  if (acceptTaskOpen && !acceptTaskOpen(mapped.task.task_id, requestId, intent)) return;
  dispatch({
    type: "snapshot",
    snapshot: mapped,
    intent,
  });
}
