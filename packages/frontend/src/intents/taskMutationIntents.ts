import {
  AppServerProtocolError,
  TASK_CANCEL,
  TASK_CLOSE_PLAN,
  TASK_QUEUE_APPEND,
  TASK_QUEUE_MOVE,
  TASK_QUEUE_REMOVE,
  TASK_QUEUE_TAKE,
  TASK_RELOAD_NATIVE_SESSION,
  TASK_SEND,
  type BackendConnection,
  type ClientInstanceId,
  type ClientMutationId,
  type ComposerMessage,
  type StateRootId,
  type TaskId,
  type QueuedMessageId,
} from "@openaide/app-server-client";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import type { AppAction, SnapshotIntent } from "../state/appReducer";
import {
  appServerAttachment,
  appServerAttachmentHandles,
  appServerComposerImages,
  localImageAttachment,
  type ComposerAttachment,
} from "../state/composerOptions";
import type { TaskComposerInput } from "../state/store";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import type { PostHostMessage } from "../state/postHostMessage";
import { isInvalidAttachmentHandleError } from "../state/attachmentValidation";
import {
  releaseComposerAttachments,
  type ComposerAttachmentResourceOwner,
} from "../services/attachmentResources";
import { composerErrorMessage } from "../components/composerDraftPolicy";

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

/** Requests one user-approved full Native Session replay. This mutation is never retried. */
export async function reloadNativeSessionIntent(
  dependencies: TaskMutationIntentDependencies,
  snapshot: TaskSnapshot | undefined,
): Promise<void> {
  if (!snapshot) throw new Error("Task snapshot unavailable.");
  const taskId = snapshot.task.task_id;
  const request = dependencies.backendConnection?.request;
  if (!request) throw new Error("App Server connection unavailable.");
  const result = await request(TASK_RELOAD_NATIVE_SESSION, {
    taskId: taskId as TaskId,
    clientMutationId: createReloadNativeSessionMutationId(),
  });
  dependencies.dispatch({
    type: "snapshot",
    snapshot: mapProtocolTaskSnapshot(result.task).snapshot,
    intent: "refresh",
  });
}

export async function closeTaskPlanIntent(
  dependencies: TaskMutationIntentDependencies,
  snapshot: TaskSnapshot | undefined,
) {
  if (!snapshot) return;
  const taskId = snapshot.task.task_id;
  if (!dependencies.backendConnection?.request) {
    dependencies.dispatch({
      type: "taskInput:error",
      taskId,
      message: "App Server connection unavailable.",
    });
    return;
  }
  try {
    const result = await dependencies.backendConnection.request(TASK_CLOSE_PLAN, {
      taskId: taskId as TaskId,
    });
    dependencies.dispatch({
      type: "snapshot",
      snapshot: mapProtocolTaskSnapshot(result.task).snapshot,
      intent: "refresh",
    });
  } catch (error) {
    dependencies.dispatch({
      type: "taskInput:error",
      taskId,
      message: taskMutationErrorMessage(error, "Unable to close Plan."),
    });
  }
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
      const acceptedSnapshot = mapProtocolTaskSnapshot(result.task).snapshot;
      dependencies.dispatch({
        type: "taskSend:accepted",
        taskId,
        userMessageId: result.userMessageId,
        snapshot: acceptedSnapshot,
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

/** Persists a follow-up without making it visible in Chat or starting a turn. */
export function appendTaskQueueIntent(
  dependencies: TaskMutationIntentDependencies,
  snapshot: TaskSnapshot | undefined,
  input: TaskComposerInput,
) {
  if (!snapshot) return;
  const taskId = snapshot.task.task_id;
  const message = appServerComposerMessage(input);
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
      message: "Reselect attachments from the file browser before queueing.",
    });
    return;
  }
  dependencies.dispatch({ type: "taskInput:submit", taskId, input });
  void dependencies.backendConnection.request(TASK_QUEUE_APPEND, {
    taskId: taskId as TaskId,
    message,
  }).then((result) => {
    dependencies.dispatch({
      type: "snapshot",
      snapshot: mapProtocolTaskSnapshot(result.task).snapshot,
      intent: "refresh",
    });
    dependencies.dispatch({
      type: "taskQueue:accepted",
      taskId,
      queueRevision: result.task.messageQueue.revision,
    });
  }).catch((error) => {
    dependencies.dispatch({
      type: "taskInput:sendError",
      taskId,
      message: taskMutationErrorMessage(error, "Unable to add message to queue."),
    });
  });
}

/** Removes one durable queued message immediately; conflicts are never retried. */
export function removeTaskQueueMessageIntent(
  dependencies: TaskMutationIntentDependencies,
  snapshot: TaskSnapshot | undefined,
  queuedMessageId: string,
) {
  if (!snapshot) return;
  const taskId = snapshot.task.task_id;
  if (!dependencies.backendConnection?.request) {
    dependencies.dispatch({
      type: "taskInput:error",
      taskId,
      message: "App Server connection unavailable.",
    });
    return;
  }
  void dependencies.backendConnection.request(TASK_QUEUE_REMOVE, {
    taskId: taskId as TaskId,
    queuedMessageId: queuedMessageId as QueuedMessageId,
    queueRevision: snapshot.message_queue?.revision ?? 0,
    clientMutationId: createQueueRemoveMutationId(),
  }).then((result) => {
    dependencies.dispatch({
      type: "snapshot",
      snapshot: mapProtocolTaskSnapshot(result.task).snapshot,
      intent: "refresh",
    });
  }).catch((error) => {
    dependencies.dispatch({
      type: "taskInput:error",
      taskId,
      message: taskMutationErrorMessage(error, "Unable to remove queued message."),
    });
  });
}

/** Atomically removes one queued message and restores it as the ordinary Composer draft. */
export function takeTaskQueueMessageIntent(
  dependencies: TaskMutationIntentDependencies,
  snapshot: TaskSnapshot | undefined,
  input: TaskComposerInput,
  queuedMessageId: string,
) {
  if (!snapshot) return;
  const taskId = snapshot.task.task_id;
  if (input.pending || input.queueTake || input.prompt.length > 0 || input.context.length > 0) {
    dependencies.dispatch({
      type: "taskInput:error",
      taskId,
      message: "Clear Composer before editing a queued message.",
    });
    return;
  }
  const queue = snapshot.message_queue;
  const index = queue?.items.findIndex((item) => item.queued_message_id === queuedMessageId) ?? -1;
  const item = index >= 0 ? queue?.items[index] : undefined;
  const request = dependencies.backendConnection?.request;
  if (!item || !request) {
    dependencies.dispatch({ type: "taskInput:error", taskId, message: "Queued message is no longer available." });
    return;
  }

  const adoption = dependencies.attachmentResources?.beginAdoption(taskId);
  if (dependencies.attachmentResources && !adoption) return;
  dependencies.dispatch({ type: "taskQueue:take:start", taskId, item, index });
  void request(TASK_QUEUE_TAKE, {
    taskId: taskId as TaskId,
    queuedMessageId: queuedMessageId as QueuedMessageId,
    queueRevision: queue?.revision ?? 0,
    clientMutationId: createQueueTakeMutationId(),
  }).then((result) => {
    const context: ComposerAttachment[] = [];
    for (const attachment of result.message.attachments ?? []) {
      if (dependencies.attachmentResources?.adopt(
        { taskId, handleId: attachment.handleId },
        adoption,
      ) === false) continue;
      context.push(appServerAttachment(attachment));
    }
    for (const image of result.message.images ?? []) {
      context.push(localImageAttachment(
        new File([], image.label, { type: image.mimeType }),
        image.data,
      ));
    }
    dependencies.dispatch({
      type: "snapshot",
      snapshot: mapProtocolTaskSnapshot(result.task).snapshot,
      intent: "refresh",
    });
    dependencies.dispatch({ type: "taskQueue:take:collapse", taskId, queuedMessageId });
    globalThis.setTimeout(() => dependencies.dispatch({
      type: "taskQueue:take:accepted",
      taskId,
      queuedMessageId,
      prompt: result.message.text,
      context,
    }), 180);
  }).catch((error) => {
    dependencies.dispatch({
      type: "taskQueue:take:error",
      taskId,
      queuedMessageId,
      message: taskMutationErrorMessage(error, "Unable to edit queued message in Composer."),
    });
  });
}

/** Moves one queued item to its final zero-based index. */
export function moveTaskQueueMessageIntent(
  dependencies: TaskMutationIntentDependencies,
  snapshot: TaskSnapshot | undefined,
  queuedMessageId: string,
  targetIndex: number,
): Promise<void> {
  return mutateTaskQueue(dependencies, snapshot, "move", {
    queuedMessageId: queuedMessageId as QueuedMessageId,
    targetIndex,
  });
}

/** Sends one observed queue item and consumes it in the App Server's Send commit. */
export function sendTaskQueueMessageNowIntent(
  dependencies: TaskMutationIntentDependencies,
  snapshot: TaskSnapshot | undefined,
  queuedMessageId: string,
) {
  if (!snapshot) return;
  const taskId = snapshot.task.task_id;
  const item = snapshot.message_queue?.items.find((candidate) => candidate.queued_message_id === queuedMessageId);
  const request = dependencies.backendConnection?.request;
  if (!item || !request) {
    dependencies.dispatch({ type: "taskInput:error", taskId, message: "Queued message is no longer available." });
    return;
  }
  void request(TASK_SEND, {
    taskId: taskId as TaskId,
    message: { text: item.text },
    queueSelection: {
      queuedMessageId: queuedMessageId as QueuedMessageId,
      queueRevision: snapshot.message_queue?.revision ?? 0,
    },
  }).then((result) => {
    const acceptedSnapshot = mapProtocolTaskSnapshot(result.task).snapshot;
    dependencies.dispatch({
      type: "taskSend:accepted",
      taskId,
      userMessageId: result.userMessageId,
      snapshot: acceptedSnapshot,
    });
  }).catch((error) => {
    dependencies.dispatch({
      type: "taskInput:error",
      taskId,
      message: taskMutationErrorMessage(error, "Unable to send queued message."),
    });
  });
}

function mutateTaskQueue(
  dependencies: TaskMutationIntentDependencies,
  snapshot: TaskSnapshot | undefined,
  kind: "move",
  mutation: { queuedMessageId: QueuedMessageId; targetIndex: number },
): Promise<void> {
  if (!snapshot) return Promise.reject(new Error("Task snapshot unavailable."));
  const taskId = snapshot.task.task_id;
  const request = dependencies.backendConnection?.request;
  if (!request) {
    const error = new Error("App Server connection unavailable.");
    dependencies.dispatch({ type: "taskInput:error", taskId, message: error.message });
    return Promise.reject(error);
  }
  const shared = {
    taskId: taskId as TaskId,
    queueRevision: snapshot.message_queue?.revision ?? 0,
    clientMutationId: createQueueMutationId(kind),
  };
  return request(TASK_QUEUE_MOVE, { ...shared, ...mutation }).then((result) => {
    dependencies.dispatch({
      type: "snapshot",
      snapshot: mapProtocolTaskSnapshot(result.task).snapshot,
      intent: "refresh",
    });
  }).catch((error) => {
    dependencies.dispatch({
      type: "taskInput:error",
      taskId,
      message: taskMutationErrorMessage(error, `Unable to ${kind} queued message.`),
    });
    throw error;
  });
}

let nextQueueRemoveMutationId = 1;
let nextQueueMutationId = 1;
let nextQueueTakeMutationId = 1;
let nextReloadNativeSessionMutationId = 1;

function createReloadNativeSessionMutationId(): ClientMutationId {
  const id = `frontend-native-session-reload-${nextReloadNativeSessionMutationId}`;
  nextReloadNativeSessionMutationId += 1;
  return id as ClientMutationId;
}

function createQueueRemoveMutationId(): ClientMutationId {
  const id = `frontend-queue-remove-${nextQueueRemoveMutationId}`;
  nextQueueRemoveMutationId += 1;
  return id as ClientMutationId;
}

function createQueueMutationId(kind: "move"): ClientMutationId {
  const id = `frontend-queue-${kind}-${nextQueueMutationId}`;
  nextQueueMutationId += 1;
  return id as ClientMutationId;
}

function createQueueTakeMutationId(): ClientMutationId {
  const id = `frontend-queue-take-${nextQueueTakeMutationId}`;
  nextQueueTakeMutationId += 1;
  return id as ClientMutationId;
}

function appServerComposerMessage(input: TaskComposerInput): ComposerMessage | undefined {
  const imageAttachments = input.context.filter((attachment) => attachment.kind === "image");
  const resourceAttachments = input.context.filter((attachment) => attachment.kind !== "image");
  const images = appServerComposerImages(imageAttachments);
  const attachments = appServerAttachmentHandles(resourceAttachments);
  if (!images || !attachments) return undefined;
  return {
    text: input.prompt,
    ...(images.length ? { images } : {}),
    ...(attachments.length ? { attachments } : {}),
  };
}

function taskSendErrorMessage(error: unknown) {
  return taskMutationErrorMessage(error, "Unable to send message.");
}

function taskMutationErrorMessage(error: unknown, fallback: string) {
  return composerErrorMessage(error, fallback);
}
