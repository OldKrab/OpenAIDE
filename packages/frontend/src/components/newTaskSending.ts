import {
  AppServerProtocolError,
  TASK_OPEN,
  TASK_SEND,
  type BackendConnection,
  type ComposerMessage,
  type TaskId,
  type TaskSendIdempotencyKey,
  type TaskSnapshot as ProtocolTaskSnapshot,
} from "@openaide/app-server-client";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import type { AppAction } from "../state/appReducer";

export async function sendNewTaskMessageWithFreshRevision({
  dispatch,
  idempotencyKey,
  message,
  request,
  taskId,
  taskRevision,
}: {
  dispatch: (action: AppAction) => void;
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
    const refreshed = await waitUntilTaskSendReady(
      request,
      (await request(TASK_OPEN, { taskId })).task,
      dispatch,
    );
    return request(TASK_SEND, {
      taskId,
      taskRevision: refreshed.revision,
      idempotencyKey,
      message,
    });
  }
}

export async function waitUntilTaskSendReady(
  request: NonNullable<BackendConnection["request"]>,
  task: ProtocolTaskSnapshot,
  dispatch: (action: AppAction) => void,
): Promise<ProtocolTaskSnapshot> {
  let current = task;
  for (let attempt = 0; attempt < 30 && current.sendCapability.state !== "ready"; attempt += 1) {
    await delay(1_000);
    current = (await request(TASK_OPEN, { taskId: current.task.taskId })).task as typeof current;
    dispatchTaskSnapshot(dispatch, current, "refresh");
  }
  return current;
}

function dispatchTaskSnapshot(
  dispatch: (action: AppAction) => void,
  task: ProtocolTaskSnapshot,
  intent: "open" | "refresh",
) {
  dispatch({ type: "snapshot", snapshot: mapProtocolTaskSnapshot(task).snapshot, intent });
}

function delay(ms: number) {
  return new Promise((resolve) => globalThis.setTimeout(resolve, ms));
}

function isTaskChangedBeforeSend(error: unknown) {
  return error instanceof AppServerProtocolError && error.protocolError.code === "conflict";
}
