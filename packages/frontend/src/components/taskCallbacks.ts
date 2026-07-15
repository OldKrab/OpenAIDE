import {
  ATTACHMENT_CREATE_FILE_REFERENCE,
  ATTACHMENT_CREATE_PASTED_IMAGE,
  ATTACHMENT_LIST_DIRECTORY,
  ATTACHMENT_LIST_ROOTS,
  ATTACHMENT_REVEAL,
  TASK_CHAT_PAGE,
  TASK_SET_CONFIG_OPTION,
  type AgentConfigOptionId,
  type BackendConnection,
  type ClientMutationId,
  type ClientInstanceId,
  type FileBrowserEntryId,
  type MessageId,
  type StateRootId,
  type FileBrowserRootId,
  type TaskId,
} from "@openaide/app-server-client";
import { postHostMessage } from "../services/hostBridge";
import { startAppServerStateSubscription } from "../services/appServerStateSubscriptions";
import {
  attachmentHandleResource,
  releaseAttachmentResources,
} from "../services/attachmentResources";
import { createConfirmedEmbeddedAttachment } from "../services/embeddedAttachmentSelection";
import { cancelTaskIntent, sendTaskPromptIntent } from "../intents/taskMutationIntents";
import { respondToPermissionIntent, respondToQuestionIntent } from "../intents/taskIntents";
import { appServerAttachment } from "../state/composerOptions";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import { configOptionsMutable } from "../state/configOptionState";
import { mapProtocolChatPage } from "../state/taskReadMapping";
import type { AppCallbacksDependencies, TaskCallbacks } from "./appControllerCallbackTypes";
import { refreshTaskSnapshotAfterMutationFailure } from "./taskSnapshotRefresh";

type TaskDependencies = Pick<
  AppCallbacksDependencies,
  | "attachmentResources"
  | "asyncOperations"
  | "backendConnection"
  | "clientInstanceId"
  | "createSnapshotRequestId"
  | "dispatch"
  | "state"
>;

type TaskBackendConnection = Partial<Pick<BackendConnection, "handleNotification" | "request">>;

export function createTaskCallbacks({
  attachmentResources,
  asyncOperations,
  backendConnection,
  clientInstanceId,
  createSnapshotRequestId,
  dispatch,
  state,
}: TaskDependencies): TaskCallbacks {
  return {
    cancel: () => {
      const cancel = () => cancelTaskIntent(
        {
          backendConnection,
          clientInstanceId,
          createSnapshotRequestId,
          dispatch,
          postHostMessage,
          stateRootId: state.appServerStateRootId,
        },
        state.snapshot,
      );
      cancel();
    },
    fileBrowser: createTaskFileBrowserCallbacks(backendConnection, dispatch, state, attachmentResources),
    loadChatPage: (beforeCursor) => {
      if (!state.snapshot) return;
      const task = state.snapshot.task;
      const operation = asyncOperations.claim(`chat-page:${task.task_id}`);
      const requestGeneration = operation.id;
      dispatch({ type: "chatPage:start", taskId: task.task_id, requestGeneration });
      if (!backendConnection?.request) {
        dispatch({
          type: "chatPage:error",
          taskId: task.task_id,
          requestGeneration,
          message: appServerRequiredMessage(),
        });
        return undefined;
      }
      void backendConnection.request(TASK_CHAT_PAGE, {
        taskId: task.task_id as TaskId,
        beforeCursor: beforeCursor as MessageId,
        limit: 50,
      })
        .then((page) => {
          if (!asyncOperations.owns(operation)) return;
          dispatch({
            type: "chatPage:result",
            taskId: task.task_id,
            requestGeneration,
            page: mapProtocolChatPage(page, task.updated_at),
          });
        })
        .catch((error) => {
          if (!asyncOperations.owns(operation)) return;
          dispatch({
            type: "chatPage:error",
            taskId: task.task_id,
            requestGeneration,
            message: safeErrorMessage(error),
          });
        });
      return requestGeneration;
    },
    subscribeToolDetail: (artifactId) => {
      if (!state.snapshot) return () => undefined;
      const taskId = state.snapshot.task.task_id;
      if (!backendConnection?.request || !backendConnection.handleNotification || !state.appServerStateRootId) {
        dispatch({ type: "toolDetail:error", taskId, artifactId, message: appServerRequiredMessage() });
        return () => undefined;
      }
      dispatch({ type: "toolDetail:start", taskId, artifactId });
      return startAppServerStateSubscription({
        backendConnection: {
          handleNotification: backendConnection.handleNotification,
          request: backendConnection.request,
        },
        context: {
          stateRootId: state.appServerStateRootId as StateRootId,
          clientInstanceId: clientInstanceId as ClientInstanceId,
        },
        dispatch,
        onBaselineError: (error) => dispatch({
          type: "toolDetail:error",
          taskId,
          artifactId,
          message: safeErrorMessage(error),
        }),
        scope: { kind: "toolDetail", taskId: taskId as TaskId, artifactId },
      });
    },
    removeAttachment: (attachmentId) => {
      if (!state.snapshot) return;
      const taskId = state.snapshot.task.task_id;
      const attachment = state.taskInputs[taskId]?.context.find((item) => item.local_id === attachmentId);
      dispatch({ type: "taskInput:attachment:remove", taskId, attachmentId });
      if (attachment?.app_server_handle_id && attachmentResources) {
        attachmentResources.release({ taskId, handleId: attachment.app_server_handle_id });
        return;
      }
      releaseAttachmentResources(
        backendConnection,
        taskId,
        attachment?.app_server_handle_id
          ? [attachmentHandleResource(attachment.app_server_handle_id)]
          : [],
      );
    },
    revealAttachment: (attachmentId) => {
      if (!state.snapshot || !backendConnection?.request) return Promise.reject(new Error(appServerRequiredMessage()));
      const taskId = state.snapshot.task.task_id;
      const attachment = state.taskInputs[taskId]?.context.find((item) => item.local_id === attachmentId);
      if (!attachment?.app_server_handle_id) return Promise.reject(new Error("Attachment handle unavailable"));
      return backendConnection
        .request(ATTACHMENT_REVEAL, {
          taskId: taskId as TaskId,
          handleId: attachment.app_server_handle_id,
        })
        .then(() => undefined);
    },
    respondToPermission: (requestId, optionId) => {
      respondToPermissionIntent(
        {
          backendConnection: backendConnection?.request
            ? { request: backendConnection.request }
            : undefined,
          dispatch,
          state,
        },
        requestId,
        optionId,
      );
    },
    respondToQuestion: (requestId, response) => {
      respondToQuestionIntent({
        backendConnection: backendConnection?.request
          ? { request: backendConnection.request }
          : undefined,
        dispatch,
        state,
      }, requestId, response);
    },
    sendPrompt: (prompt) => {
      if (!state.snapshot) return;
      const taskId = state.snapshot.task.task_id;
      const taskInput = state.taskInputs[taskId] ?? { prompt: "", context: [] };
      const input = prompt === undefined ? taskInput : { ...taskInput, prompt };
      sendTaskPromptIntent(
        {
          attachmentResources,
          backendConnection,
          clientInstanceId,
          createSnapshotRequestId,
          dispatch,
          postHostMessage,
          stateRootId: state.appServerStateRootId,
        },
        state.snapshot,
        input,
      );
    },
    selectConfigOption: (configId, value) => {
      if (!state.snapshot) return;
      const taskId = state.snapshot.task.task_id;
      const catalog = state.snapshot.agent_config;
      if (!configOptionsMutable(state.snapshot.agent_config)) {
        dispatch({
          type: "taskInput:error",
          taskId,
          message: "Configuration options are not currently editable.",
        });
        return;
      }
      const request = backendConnection?.request;
      if (!request) {
        dispatch({ type: "taskInput:error", taskId, message: appServerRequiredMessage() });
        return;
      }
      const mutationId = createTaskConfigMutationId(configId);
      void request(TASK_SET_CONFIG_OPTION, {
        taskId: taskId as TaskId,
        configId: configId as AgentConfigOptionId,
        value,
        clientMutationId: mutationId,
      })
        .then((result) => {
          // The request result remains authoritative if the event stream is interrupted.
          // Revision-aware ingestion makes a duplicate event/result pair idempotent.
          dispatch({ type: "snapshot", snapshot: mapProtocolTaskSnapshot(result.task).snapshot, intent: "refresh" });
        })
        .catch((error) => {
          dispatch({
            type: "taskInput:configError",
            taskId,
            mutationId,
            message: safeErrorMessage(error),
            catalog,
          });
          globalThis.setTimeout(() => {
            dispatch({ type: "taskInput:configError:clear", taskId, mutationId });
          }, 10_000);
          void refreshTaskSnapshotAfterMutationFailure({
            dispatch,
            request,
            taskId,
          });
        });
    },
  };
}

function createTaskFileBrowserCallbacks(
  backendConnection: TaskBackendConnection | undefined,
  dispatch: TaskDependencies["dispatch"],
  state: TaskDependencies["state"],
  attachmentResources: TaskDependencies["attachmentResources"],
) {
  const request = backendConnection?.request;
  const taskId = state.snapshot?.task.task_id;
  if (!request || !taskId) return undefined;
  return {
    ownerKey: `task:${taskId}`,
    listRoots: async () => {
      const result = await request(ATTACHMENT_LIST_ROOTS, { taskId: taskId as TaskId });
      return result.roots;
    },
    listDirectory: (rootId: FileBrowserRootId, directoryId?: FileBrowserEntryId) =>
      request(ATTACHMENT_LIST_DIRECTORY, {
        taskId: taskId as TaskId,
        rootId,
        directoryId,
    }),
    attachFileReference: async (entryId: FileBrowserEntryId) => {
      const adoption = attachmentResources?.beginAdoption(taskId);
      if (attachmentResources && !adoption) return;
      const result = await request(ATTACHMENT_CREATE_FILE_REFERENCE, {
        taskId: taskId as TaskId,
        entryId,
      });
      if (attachmentResources?.adopt({ taskId, handleId: result.attachment.handleId }, adoption) === false) return;
      dispatch({
        type: "taskInput:attachment:addAppServer",
        taskId,
        attachment: appServerAttachment(result.attachment),
      });
    },
    attachPastedImage: async (file: File) => {
      const adoption = attachmentResources?.beginAdoption(taskId);
      if (attachmentResources && !adoption) return;
      const data = await fileToBase64(file);
      const previewUrl = `data:${file.type || "image/png"};base64,${data}`;
      const result = await request(ATTACHMENT_CREATE_PASTED_IMAGE, {
        taskId: taskId as TaskId,
        label: file.name || "Pasted image",
        mimeType: file.type || "image/png",
        data,
      });
      if (attachmentResources?.adopt({ taskId, handleId: result.attachment.handleId }, adoption) === false) return;
      dispatch({
        type: "taskInput:attachment:addAppServer",
        taskId,
        attachment: appServerAttachment(result.attachment, { previewUrl }),
      });
    },
    attachEmbedded: async (entryId: FileBrowserEntryId) => {
      const adoption = attachmentResources?.beginAdoption(taskId);
      if (attachmentResources && !adoption) return;
      const attachment = await createConfirmedEmbeddedAttachment(
        { request },
        taskId as TaskId,
        entryId,
        () => attachmentAdoptionDisposition(attachmentResources, adoption),
      );
      if (attachmentResources?.adopt({ taskId, handleId: attachment.handleId }, adoption) === false) return;
      dispatch({
        type: "taskInput:attachment:addAppServer",
        taskId,
        attachment: appServerAttachment(attachment),
      });
    },
  };
}

function attachmentAdoptionDisposition(
  attachmentResources: TaskDependencies["attachmentResources"],
  adoption: ReturnType<NonNullable<TaskDependencies["attachmentResources"]>["beginAdoption"]>,
) {
  if (!attachmentResources || !adoption) return "current" as const;
  const status = attachmentResources.adoptionStatus(adoption);
  if (status === "replacedReplica") return "forget" as const;
  return status === "current" ? "current" as const : "release" as const;
}

async function fileToBase64(file: File) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  const chunkSize = 0x8000;
  for (let index = 0; index < bytes.length; index += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(index, index + chunkSize));
  }
  return btoa(binary);
}

function safeErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Task read request failed";
}

function appServerRequiredMessage() {
  return "App Server connection unavailable.";
}

let nextTaskConfigMutationId = 1;

function createTaskConfigMutationId(configId: string): ClientMutationId {
  const id = `frontend-task-config-${configId}-${nextTaskConfigMutationId}`;
  nextTaskConfigMutationId += 1;
  return id as ClientMutationId;
}
