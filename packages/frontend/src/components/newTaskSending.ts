import {
  AppServerProtocolError,
  TASK_OPEN,
  TASK_SEND,
  type BackendConnection,
  type ComposerMessage,
  type TaskId,
  type TaskSendIdempotencyKey,
} from "@openaide/app-server-client";

export async function sendNewTaskMessageWithFreshRevision({
  idempotencyKey,
  message,
  request,
  taskId,
  taskRevision,
}: {
  idempotencyKey: TaskSendIdempotencyKey;
  message: ComposerMessage;
  request: NonNullable<BackendConnection["request"]>;
  taskId: TaskId;
  taskRevision: number;
}) {
  try {
    return await request(TASK_SEND, {
      taskId,
      taskRevision,
      idempotencyKey,
      message,
    });
  } catch (error) {
    if (!isTaskChangedBeforeSend(error)) throw error;
    const refreshed = (await request(TASK_OPEN, { taskId })).task;
    return request(TASK_SEND, {
      taskId,
      taskRevision: refreshed.revision,
      idempotencyKey,
      message,
    });
  }
}

function isTaskChangedBeforeSend(error: unknown) {
  return error instanceof AppServerProtocolError && error.protocolError.code === "conflict";
}
