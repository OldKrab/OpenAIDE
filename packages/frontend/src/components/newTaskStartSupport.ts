import {
  TASK_CANCEL,
  TASK_DISCARD,
  type BackendConnection,
  type ClientMutationId,
  type TaskId,
} from "@openaide/app-server-client";
import type { AppState } from "../state/store";
import type { NewTaskDraftInput } from "./appControllerCallbackTypes";

/** Stops a pre-send Task, falling through to turn cancellation if send won the race. */
export async function discardOrCancelStartedTask(
  request: NonNullable<BackendConnection["request"]>,
  taskId: TaskId,
) {
  try {
    await request(TASK_DISCARD, { taskId });
  } catch {
    await request(TASK_CANCEL, { taskId });
  }
}

export async function fileToBase64(file: File) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  const chunkSize = 0x8000;
  for (let index = 0; index < bytes.length; index += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(index, index + chunkSize));
  }
  return btoa(binary);
}

let nextNewTaskMutationId = 1;

export function createNewTaskMutationId(configId: string): ClientMutationId {
  const id = `frontend-new-task-${configId}-${nextNewTaskMutationId}`;
  nextNewTaskMutationId += 1;
  return id as ClientMutationId;
}

export function submitErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Unable to start task.";
}

export function newTaskDraftInput(state: AppState, draft?: NewTaskDraftInput) {
  const preparedTaskId = state.snapshot && !state.snapshot.task.has_messages
    ? state.snapshot.task.task_id
    : undefined;
  const preparedInput = preparedTaskId ? state.taskInputs[preparedTaskId] : undefined;
  if (preparedInput) {
    return draft ? { ...preparedInput, prompt: draft.prompt, context: draft.context } : preparedInput;
  }
  return draft ?? { prompt: state.newTask.prompt, context: state.newTask.context };
}

export function shouldPreservePendingSendRecovery() {
  return typeof document !== "undefined" && document.visibilityState === "hidden";
}
