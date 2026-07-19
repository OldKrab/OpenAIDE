import {
  AppServerProtocolError,
  TASK_RELEASE,
  type BackendConnection,
  type TaskId,
} from "@openaide/app-server-client";

/** Resolves only the App Server-identified lease conflict, then retries exactly once. */
export async function acquirePreparedTaskWithConflictRetry<T>(
  request: BackendConnection["request"],
  acquire: () => Promise<T>,
): Promise<T> {
  try {
    return await acquire();
  } catch (error) {
    const taskId = conflictingPreparedTaskId(error);
    if (!taskId) throw error;
    const released = await request(TASK_RELEASE, { taskId });
    if (released.taskId !== taskId) {
      throw new Error(`Release acknowledged ${released.taskId} instead of ${taskId}`);
    }
    return acquire();
  }
}

function conflictingPreparedTaskId(error: unknown): TaskId | undefined {
  if (!(error instanceof AppServerProtocolError) || error.protocolError.code !== "conflict") {
    return undefined;
  }
  const currentTask = error.protocolError.target?.currentTask;
  if (currentTask?.lifecycle !== "new") return undefined;
  return currentTask.task.taskId;
}
