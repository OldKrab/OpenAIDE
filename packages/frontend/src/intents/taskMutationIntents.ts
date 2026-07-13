import {
  AppServerProtocolError,
  TASK_CANCEL,
  TASK_SEND,
  type BackendConnection,
  type ClientInstanceId,
  type ComposerMessage,
  type StateRootId,
  type TaskId,
} from "@openaide/app-server-client";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import type { AppAction, SnapshotIntent } from "../state/appReducer";
import { appServerAttachmentHandles } from "../state/composerOptions";
import type { TaskComposerInput } from "../state/store";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import type { PostHostMessage } from "../state/postHostMessage";
import { isInvalidAttachmentHandleError } from "../state/attachmentValidation";
import {
  releaseComposerAttachments,
  type ComposerAttachmentResourceOwner,
} from "../services/attachmentResources";

type TaskMutationConnection = Pick<BackendConnection, "request">;

export type TaskMutationIntentDependencies = {
  attachmentResources?: ComposerAttachmentResourceOwner;
  backendConnection?: Partial<TaskMutationConnection>;
  clientInstanceId: ClientInstanceId | string;
  createSnapshotRequestId: (taskId?: string, intent?: SnapshotIntent) => number;
  dispatch: (action: AppAction) => void;
  postHostMessage: PostHostMessage;
  stateRootId: StateRootId | string | undefined;
};

export function cancelTaskIntent(
  dependencies: TaskMutationIntentDependencies,
  snapshot: TaskSnapshot | undefined,
) {
  if (!snapshot) return;
  const taskId = snapshot.task.task_id;
  if (!dependencies.backendConnection?.request) {
    dependencies.dispatch({
      type: "taskInput:cancelError",
      taskId,
      message: "App Server connection unavailable.",
    });
    return;
  }
  void dependencies.backendConnection
    .request(TASK_CANCEL, { taskId: taskId as TaskId })
    .then((result) => {
      dependencies.dispatch({
        type: "snapshot",
        snapshot: mapProtocolTaskSnapshot(result.task).snapshot,
        intent: "refresh",
      });
    })
    .catch((error) => {
      dependencies.dispatch({
        type: "taskInput:cancelError",
        taskId,
        message: taskMutationErrorMessage(error, "Unable to stop task."),
      });
    });
}

export function sendTaskPromptIntent(
  dependencies: TaskMutationIntentDependencies,
  snapshot: TaskSnapshot | undefined,
  input: TaskComposerInput,
) {
  if (!snapshot) return;
  const message = appServerComposerMessage(input);
  const taskId = snapshot.task.task_id;
  if (snapshot.send_capability.state !== "ready") {
    dependencies.dispatch({
      type: "taskInput:error",
      taskId,
      message: snapshot.send_capability.blockers?.[0]?.message ?? "Task is not ready to accept a message.",
    });
    return;
  }
  if (!dependencies.backendConnection?.request) {
    dependencies.dispatch({
      type: "taskInput:error",
      taskId,
      message: "App Server connection unavailable.",
    });
    return;
  }
  if (!message) {
    dependencies.dispatch({
      type: "taskInput:error",
      taskId,
      message: "Reselect attachments from the file browser before sending.",
    });
    return;
  }
  dependencies.attachmentResources?.lockAdoptions();
  dependencies.dispatch({
    type: "taskInput:submit",
    taskId,
    input,
  });
  void dependencies.backendConnection.request(TASK_SEND, {
    taskId: taskId as TaskId,
    message,
  })
    .then((result) => {
      dependencies.dispatch({
        type: "snapshot",
        snapshot: mapProtocolTaskSnapshot(result.task).snapshot,
        intent: "refresh",
      });
      dependencies.dispatch({
        type: "taskSend:accepted",
        taskId,
        userMessageId: result.userMessageId,
      });
    })
    .catch((error) => {
      if (isInvalidAttachmentHandleError(error)) {
        releaseComposerAttachments({
          attachmentResources: dependencies.attachmentResources,
          attachments: input.context,
          backendConnection: dependencies.backendConnection,
          taskId,
        });
        dependencies.dispatch({
          type: "taskInput:attachments:invalidate",
          taskId,
          message: error.message,
        });
        return;
      }
      dependencies.dispatch({
        type: "taskInput:sendError",
        taskId,
        message: taskSendErrorMessage(error),
      });
    });
}

function appServerComposerMessage(input: TaskComposerInput): ComposerMessage | undefined {
  const attachments = appServerAttachmentHandles(input.context);
  if (input.context.length > 0 && !attachments) return undefined;
  return attachments?.length ? { text: input.prompt, attachments } : { text: input.prompt };
}

function taskSendErrorMessage(error: unknown) {
  return taskMutationErrorMessage(error, "Unable to send message.");
}

function taskMutationErrorMessage(error: unknown, fallback: string) {
  if (error instanceof AppServerProtocolError) return error.message;
  return error instanceof Error ? error.message : fallback;
}
