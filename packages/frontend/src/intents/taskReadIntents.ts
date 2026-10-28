import {
  TASK_ARCHIVE,
  TASK_COMPOSER_HISTORY,
  TASK_OPEN,
  TASK_RESTORE,
  type BackendConnection,
  type ComposerHistoryScope,
  type TaskId,
} from "@openaide/app-server-client";
import type { AppAction, SnapshotIntent } from "../state/appReducer";

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

/** Reads the App Server-owned recall projection without deriving history from rendered Chat. */
export async function requestComposerHistory(
  backendConnection: TaskReadConnection | undefined,
  scope: ComposerHistoryScope,
) {
  if (!backendConnection) return [];
  const result = await backendConnection.request(TASK_COMPOSER_HISTORY, { scope });
  return result.entries.map((entry) => entry.text);
}

/**
 * Opens a Task and selects it once the App Server acknowledges the open. The reply
 * carries no Task state (ADR-0074): the Task subscription supplies the snapshot.
 */
export async function requestTaskOpen(
  { acceptTaskOpen, backendConnection, createTaskOpenRequestId, dispatch }: TaskReadIntentDependencies,
  taskId: string,
) {
  const requestId = createTaskOpenRequestId?.(taskId, "open");
  await backendConnection.request(TASK_OPEN, { taskId: taskId as TaskId });
  if (acceptTaskOpen && !acceptTaskOpen(taskId, requestId, "open")) return;
  dispatch({ type: "taskOpen:accepted", taskId });
}
