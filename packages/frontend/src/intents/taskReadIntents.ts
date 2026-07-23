import {
  TASK_ARCHIVE,
  TASK_OPEN,
  TASK_RESTORE,
  type BackendConnection,
  type TaskId,
  type TaskOpenResult,
} from "@openaide/app-server-client";
import type { AppAction, SnapshotIntent } from "../state/appReducer";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";

type TaskReadConnection = Pick<BackendConnection, "request">;

export type TaskReadIntentDependencies = {
  acceptTaskOpen?: (taskId: string, requestId: number | undefined, intent: SnapshotIntent) => boolean;
  backendConnection: TaskReadConnection;
  createTaskOpenRequestId?: (taskId: string, intent: SnapshotIntent) => number;
  dispatch: (action: AppAction) => void;
};

export async function requestTaskArchive(
  { backendConnection }: TaskReadIntentDependencies,
  taskId: string,
) {
  await backendConnection.request(TASK_ARCHIVE, { taskId: taskId as TaskId });
}

export async function requestTaskRestore(
  { backendConnection }: TaskReadIntentDependencies,
  taskId: string,
) {
  await backendConnection.request(TASK_RESTORE, { taskId: taskId as TaskId });
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
