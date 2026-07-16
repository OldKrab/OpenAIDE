import {
  AppServerProtocolError,
  TASK_CANCEL,
  TASK_RELEASE,
  type BackendConnection,
  type TaskId,
} from "@openaide/app-server-client";
import type { AppState } from "../state/store";
import type { NewTaskDraftInput } from "./appControllerCallbackTypes";
import { preparedTaskMatchesNewTaskContext } from "../state/newTaskPreparationContext";

/** Stops a pre-send Task, falling through to turn cancellation if send won the race. */
export async function discardOrCancelStartedTask(
  request: NonNullable<BackendConnection["request"]>,
  taskId: TaskId,
) {
  try {
    await request(TASK_RELEASE, { taskId });
    return "discarded" as const;
  } catch (error) {
    if (!(error instanceof AppServerProtocolError) || error.protocolError.code !== "conflict") {
      throw error;
    }
    await request(TASK_CANCEL, { taskId });
    return "cancelled" as const;
  }
}

export function submitErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Unable to start task.";
}

export function newTaskDraftInput(state: AppState, draft?: NewTaskDraftInput) {
  const preparedTask = state.snapshot?.task;
  const preparedTaskId = preparedTask
    && !preparedTask.has_messages
    && preparedTaskMatchesNewTaskContext(state, {
      agentId: preparedTask.agent_id,
      projectId: preparedTask.project_id,
      workspaceRoot: preparedTask.workspace_root,
    })
    ? preparedTask.task_id
    : undefined;
  const preparedInput = preparedTaskId ? state.taskInputs[preparedTaskId] : undefined;
  if (preparedInput) {
    return draft ? { ...preparedInput, prompt: draft.prompt, context: draft.context } : preparedInput;
  }
  return draft ?? { prompt: state.newTask.prompt, context: state.newTask.context };
}
