import {
  AppServerProtocolError,
  TASK_SEND,
  type BackendConnection,
  type ClientInstanceId,
  type ComposerMessage,
  type StateRootId,
  type TaskId,
  type TaskSendIdempotencyKey,
} from "@openaide/app-server-client";
import type { ComposerAttachment } from "../state/composerOptions";
import {
  clearPendingTaskSendRecovery,
  readPendingTaskSendRecovery,
  savePendingTaskSendRecovery,
  type PendingTaskSendRecovery,
} from "./pendingTaskSendRecovery";

type TaskSendRequest = Pick<BackendConnection, "request">;
export type TaskSendExecution = {
  attempt: PendingTaskSendRecovery;
  result: Awaited<ReturnType<typeof send>>;
};

const inFlightTaskSends = new Map<string, {
  attempt: PendingTaskSendRecovery;
  promise: Promise<TaskSendExecution>;
}>();

export const TASK_SEND_OUTCOME_UNKNOWN_MESSAGE =
  "Send status is unknown. Retry sends this exact message.";

export function resolveTaskSendAttempt({
  clientInstanceId,
  idempotencyKey,
  message,
  renderState,
  stateRootId,
  taskId,
  taskRevision,
}: PendingTaskSendRecovery): PendingTaskSendRecovery {
  const existing = readPendingTaskSendRecovery(stateRootId, clientInstanceId, taskId);
  if (!existing) {
    return { clientInstanceId, idempotencyKey, message, renderState, stateRootId, taskId, taskRevision };
  }
  // The idempotency key owns the message after task/send starts. A later draft
  // must restore that exact attempt instead of replacing or stranding it.
  return existing;
}

/** Keeps one idempotency key durable until task/send returns an authoritative response. */
export function executeTaskSendAttempt({
  attempt,
  backendConnection,
  refreshRevisionOnConflict = false,
}: {
  attempt: PendingTaskSendRecovery;
  backendConnection: TaskSendRequest;
  refreshRevisionOnConflict?: boolean;
}) {
  const key = inFlightTaskSendKey(attempt);
  const inFlight = inFlightTaskSends.get(key);
  if (inFlight) return inFlight.promise;

  const execution = runTaskSendAttempt({ attempt, backendConnection, refreshRevisionOnConflict });
  const tracked = execution.finally(() => {
    if (inFlightTaskSends.get(key)?.promise === tracked) {
      inFlightTaskSends.delete(key);
    }
  });
  inFlightTaskSends.set(key, { attempt, promise: tracked });
  return tracked;
}

/** Returns the already-running exact send without starting or recovering one. */
export function inFlightTaskSendAttempt({
  clientInstanceId,
  idempotencyKey,
  stateRootId,
  taskId,
}: {
  clientInstanceId: ClientInstanceId | string;
  idempotencyKey: TaskSendIdempotencyKey;
  stateRootId: StateRootId | string;
  taskId: TaskId | string;
}): Promise<TaskSendExecution> | undefined {
  return inFlightTaskSends.get(taskSendKey(stateRootId, clientInstanceId, taskId, idempotencyKey))?.promise;
}

async function runTaskSendAttempt({
  attempt,
  backendConnection,
  refreshRevisionOnConflict,
}: {
  attempt: PendingTaskSendRecovery;
  backendConnection: TaskSendRequest;
  refreshRevisionOnConflict: boolean;
}): Promise<TaskSendExecution> {
  let current = attempt;
  savePendingTaskSendRecovery(current);
  try {
    let result;
    try {
      result = await send(backendConnection, current);
    } catch (error) {
      const refreshed = refreshRevisionOnConflict
        ? currentTaskFromRevisionConflict(error, current.taskId)
        : undefined;
      if (!refreshed) throw error;
      current = { ...current, taskRevision: refreshed.revision };
      savePendingTaskSendRecovery(current);
      result = await send(backendConnection, current);
    }
    clearPendingTaskSendRecovery(
      current.stateRootId,
      current.clientInstanceId,
      current.taskId,
      current.idempotencyKey,
    );
    return { attempt: current, result };
  } catch (error) {
    // A protocol error is an authoritative rejection. Transport failures remain
    // ambiguous and keep the exact attempt available for same-client retry.
    if (error instanceof AppServerProtocolError) {
      clearPendingTaskSendRecovery(
        current.stateRootId,
        current.clientInstanceId,
        current.taskId,
        current.idempotencyKey,
      );
    }
    throw error;
  }
}

function inFlightTaskSendKey(attempt: PendingTaskSendRecovery) {
  return taskSendKey(
    attempt.stateRootId,
    attempt.clientInstanceId,
    attempt.taskId,
    attempt.idempotencyKey,
  );
}

function taskSendKey(
  stateRootId: StateRootId | string,
  clientInstanceId: ClientInstanceId | string,
  taskId: TaskId | string,
  idempotencyKey: TaskSendIdempotencyKey,
) {
  return `${stateRootId}\u0000${clientInstanceId}\u0000${taskId}\u0000${idempotencyKey}`;
}

export function taskSendAttemptRecord({
  clientInstanceId,
  idempotencyKey,
  message,
  renderState,
  stateRootId,
  taskId,
  taskRevision,
}: {
  clientInstanceId: ClientInstanceId | string;
  idempotencyKey: TaskSendIdempotencyKey;
  message: ComposerMessage;
  renderState: { prompt: string; context: ComposerAttachment[] };
  stateRootId: StateRootId | string;
  taskId: string;
  taskRevision: number;
}): PendingTaskSendRecovery {
  return {
    clientInstanceId: clientInstanceId as ClientInstanceId,
    idempotencyKey,
    message,
    renderState,
    stateRootId: stateRootId as StateRootId,
    taskId,
    taskRevision,
  };
}

function send(backendConnection: TaskSendRequest, attempt: PendingTaskSendRecovery) {
  return backendConnection.request(TASK_SEND, {
    taskId: attempt.taskId as TaskId,
    taskRevision: attempt.taskRevision,
    idempotencyKey: attempt.idempotencyKey,
    message: attempt.message,
  });
}

function currentTaskFromRevisionConflict(error: unknown, taskId: string) {
  if (!(error instanceof AppServerProtocolError)
    || error.protocolError.code !== "conflict"
    || error.protocolError.target?.field !== "taskRevision"
    || error.protocolError.target.currentTask?.task.taskId !== taskId) {
    return undefined;
  }
  return error.protocolError.target.currentTask;
}

/** Non-protocol failures cannot prove whether App Server accepted task/send. */
export function isTaskSendOutcomeUnknown(error: unknown) {
  return !(error instanceof AppServerProtocolError);
}
