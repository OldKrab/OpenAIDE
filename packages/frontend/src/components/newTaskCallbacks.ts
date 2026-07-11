import {
  AGENT_SET_CONFIG_OPTION,
  ATTACHMENT_CONFIRM_EMBEDDED,
  ATTACHMENT_CREATE_EMBEDDED_CANDIDATE,
  ATTACHMENT_CREATE_FILE_REFERENCE,
  ATTACHMENT_CREATE_PASTED_IMAGE,
  ATTACHMENT_LIST_DIRECTORY,
  ATTACHMENT_LIST_ROOTS,
  TASK_CREATE,
  TASK_CANCEL,
  TASK_OPEN,
  TASK_SET_CONFIG_OPTION,
  WORKSPACE_LIST_DIRECTORY,
  WORKSPACE_LIST_ROOTS,
  type AgentConfigOptionId,
  type AgentId,
  type BackendConnection,
  type FileBrowserEntryId,
  type FileBrowserRootId,
  type ProjectId,
  type TaskId,
  type TaskSnapshot as ProtocolTaskSnapshot,
} from "@openaide/app-server-client";
import { createTaskSendIdempotencyKey } from "../intents/taskMutationIntents";
import { postHostMessage } from "../services/hostBridge";
import {
  clearPendingTaskSendRecovery,
  savePendingTaskSendRecovery,
} from "../services/pendingTaskSendRecovery";
import { mapProtocolConfigOptions } from "../state/appServerConfigOptions";
import { appServerAttachment, appServerAttachmentHandles } from "../state/composerOptions";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import type { AppAction } from "../state/appReducer";
import type { AppState } from "../state/store";
import { workspaceRootForProjectId } from "../state/projectIdentity";
import { configOptionsRequestKey } from "../state/surfaceRouting";
import { isInvalidAttachmentHandleError } from "../state/attachmentValidation";
import type { AppCallbacksDependencies, NewTaskCallbacks, NewTaskDraftInput, NewTaskStartAttempt } from "./appControllerCallbackTypes";
import { newTaskPreparationKey, taskCreateParams } from "./newTaskPreparationContext";
import { sendNewTaskMessageWithFreshRevision, waitUntilTaskSendReady } from "./newTaskSending";
import {
  createNewTaskMutationId,
  discardOrCancelStartedTask,
  fileToBase64,
  newTaskDraftInput,
  shouldPreservePendingSendRecovery,
  submitErrorMessage,
} from "./newTaskStartSupport";

type NewTaskDependencies = Pick<
  AppCallbacksDependencies,
  "backendConnection" | "currentNavigationGeneration" | "dispatch" | "latestOptionsRequestKey" | "newTaskStartAttempt" | "pendingPreparedNewTask" | "state"
>;

export function createNewTaskCallbacks({
  backendConnection,
  currentNavigationGeneration,
  dispatch,
  latestOptionsRequestKey,
  newTaskStartAttempt,
  pendingPreparedNewTask,
  state,
}: NewTaskDependencies): NewTaskCallbacks {
  return {
    cancel: () => {
      const attempt = newTaskStartAttempt.current;
      if (!attempt || attempt.cancelled) return;
      attempt.cancelled = true;
      dispatch({ type: "submit:cancel" });
      postHostMessage(state.newTask.selection.projectId
        ? { type: "surface.openNewTask", payload: { project_id: state.newTask.selection.projectId } }
        : { type: "surface.openNewTask" });
      if (attempt.taskId && backendConnection?.request) {
        void discardOrCancelStartedTask(backendConnection.request, attempt.taskId).finally(() => {
          if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
        });
      }
    },
    fileBrowser: createNewTaskFileBrowserCallbacks({ backendConnection, dispatch, latestOptionsRequestKey, pendingPreparedNewTask, state }),
    resetOptionsRequestKey: () => {
      latestOptionsRequestKey.current = undefined;
    },
    selectConfigOption: (configId, value) => {
      if (backendConnection?.request) {
        const projectId = state.newTask.selection.projectId;
        if (!projectId) {
          dispatch({ type: "newTask:configOptions:error", message: "Project is not ready yet." });
          return;
        }
        const workspaceRoot = workspaceRootForProjectId(
          projectId,
          state.newTask.selection.workspaceRoot,
        );
        const key = configOptionsRequestKey(
          state.newTask.selection.agentId,
          projectId,
          workspaceRoot,
        );
        latestOptionsRequestKey.current = key;
        dispatch({ type: "newTask:configOptions:start" });
        void backendConnection.request(AGENT_SET_CONFIG_OPTION, {
          agentId: state.newTask.selection.agentId as AgentId,
          projectId: projectId as ProjectId,
          ...(workspaceRoot ? { workspaceRoot } : {}),
          configId: configId as AgentConfigOptionId,
          value,
        }).then((result) => {
          if (latestOptionsRequestKey.current !== key) return;
          dispatch({ type: "newTask:configOptions:result", catalog: mapProtocolConfigOptions(result) });
        }).catch(() => {
          if (latestOptionsRequestKey.current !== key) return;
          dispatch({ type: "newTask:configOptions:error", message: "Unable to update Agent option." });
        });
        return;
      }
      dispatch({ type: "newTask:configOptions:error", message: "App Server connection unavailable." });
    },
    submit: (draft) => {
      void submitNewTask({ backendConnection, currentNavigationGeneration, dispatch, draft, newTaskStartAttempt, pendingPreparedNewTask, state });
    },
    workspaceBrowser: createWorkspaceBrowserCallbacks({ backendConnection }),
  };
}

type NewTaskSubmitDependencies = Pick<NewTaskDependencies, "backendConnection" | "currentNavigationGeneration" | "dispatch" | "newTaskStartAttempt" | "state"> & {
  draft?: NewTaskDraftInput;
  pendingPreparedNewTask?: NewTaskDependencies["pendingPreparedNewTask"];
};
type NewTaskFileBrowserDependencies = Pick<
  NewTaskDependencies,
  "backendConnection" | "dispatch" | "latestOptionsRequestKey" | "pendingPreparedNewTask" | "state"
>;
type PreparedTaskCallback = (task: ProtocolTaskSnapshot, taskId: TaskId) => void;
type PrepareNewTaskOptions = {
  acceptPreparedTask?: () => boolean;
  preparedTask?: ProtocolTaskSnapshot;
  snapshotIntent?: "open" | "refresh";
  waitForSendReady?: boolean;
};

function createWorkspaceBrowserCallbacks({
  backendConnection,
}: Pick<NewTaskDependencies, "backendConnection">) {
  const request = backendConnection?.request;
  if (!request) return undefined;
  return {
    listRoots: async () => (await request(WORKSPACE_LIST_ROOTS, {})).roots,
    listDirectory: async (path: string) => request(WORKSPACE_LIST_DIRECTORY, { path }),
  };
}

function createNewTaskFileBrowserCallbacks({
  backendConnection,
  dispatch,
  latestOptionsRequestKey,
  pendingPreparedNewTask,
  state,
}: NewTaskFileBrowserDependencies) {
  const request = backendConnection?.request;
  if (!request) return undefined;
  let preparedTaskId = state.snapshot?.task.task_id as TaskId | undefined;
  const ensureTaskId = async (draft?: NewTaskDraftInput) => {
    if (preparedTaskId) return preparedTaskId;
    latestOptionsRequestKey.current = undefined;
    const key = newTaskPreparationKey(state);
    const pending = key ? pendingPreparedNewTask(key) : undefined;
    const prepared = pending
      ? await pending
      : await prepareNewTask({ backendConnection, dispatch, state }, { waitForSendReady: false });
    preparedTaskId = prepared.taskId;
    dispatch({ type: "taskInput:prompt", taskId: prepared.taskId, prompt: draft?.prompt ?? state.newTask.prompt });
    return preparedTaskId;
  };

  return {
    listRoots: async () => {
      const taskId = await ensureTaskId();
      const result = await request(ATTACHMENT_LIST_ROOTS, { taskId });
      return result.roots;
    },
    listDirectory: async (rootId: FileBrowserRootId, directoryId?: FileBrowserEntryId) => {
      const taskId = await ensureTaskId();
      return request(ATTACHMENT_LIST_DIRECTORY, {
        taskId,
        rootId,
        directoryId,
      });
    },
    attachFileReference: async (entryId: FileBrowserEntryId) => {
      const taskId = await ensureTaskId();
      const result = await request(ATTACHMENT_CREATE_FILE_REFERENCE, {
        taskId,
        entryId,
      });
      dispatch({
        type: "taskInput:attachment:addAppServer",
        taskId,
        attachment: appServerAttachment(result.attachment),
      });
    },
    attachPastedImage: async (file: File, draft?: NewTaskDraftInput) => {
      const taskId = await ensureTaskId(draft);
      const data = await fileToBase64(file);
      const previewUrl = `data:${file.type || "image/png"};base64,${data}`;
      const result = await request(ATTACHMENT_CREATE_PASTED_IMAGE, {
        taskId,
        label: file.name || "Pasted image",
        mimeType: file.type || "image/png",
        data,
      });
      dispatch({
        type: "taskInput:attachment:addAppServer",
        taskId,
        attachment: appServerAttachment(result.attachment, { previewUrl }),
      });
    },
    attachEmbedded: async (entryId: FileBrowserEntryId) => {
      const taskId = await ensureTaskId();
      const candidate = await request(ATTACHMENT_CREATE_EMBEDDED_CANDIDATE, {
        taskId,
        entryId,
      });
      const confirmed = await request(ATTACHMENT_CONFIRM_EMBEDDED, {
        taskId,
        candidates: [candidate.candidate.candidateId],
      });
      const error = confirmed.errors[0];
      if (error) throw new Error(error.message);
      const attachment = confirmed.attachments[0];
      if (!attachment) throw new Error("Embedded attachment was not confirmed.");
      dispatch({
        type: "taskInput:attachment:addAppServer",
        taskId,
        attachment: appServerAttachment(attachment),
      });
    },
  };
}

async function submitNewTask({ backendConnection, currentNavigationGeneration, dispatch, draft, newTaskStartAttempt, pendingPreparedNewTask, state }: NewTaskSubmitDependencies) {
  const request = backendConnection?.request;
  if (!request) {
    dispatch({ type: "submit:error", message: "App Server connection unavailable." });
    return;
  }
  const projectId = state.newTask.selection.projectId;
  if (!projectId) {
    dispatch({ type: "submit:error", message: "Project unavailable. Refresh and try again." });
    return;
  }
  const draftInput = newTaskDraftInput(state, draft);
  const attempt: NewTaskStartAttempt = { cancelled: false, draft: draftInput };
  newTaskStartAttempt.current = attempt;
  const attachments = appServerAttachmentHandles(draftInput.context);
  if (draftInput.context.length > 0 && !attachments) {
    dispatch({ type: "submit:error", message: "Reselect attachments from the file browser before sending." });
    return;
  }

  dispatch(draft
    ? { type: "submit:start", prompt: draftInput.prompt, context: draftInput.context }
    : { type: "submit:start" });
  const navigationGeneration = currentNavigationGeneration();
  let createdTaskId: TaskId | undefined;
  let openedPreparedTask = false;
  const openPreparedTask: PreparedTaskCallback = (task, taskId) => {
    openedPreparedTask = true;
    dispatch({ type: "taskInput:prompt", taskId, prompt: draftInput.prompt });
    dispatch({ type: "taskInput:submit", taskId });
    if (currentNavigationGeneration() !== navigationGeneration) return;
    postHostMessage({
      type: "surface.openTask",
      payload: {
        task_id: taskId,
        title: task.task.title,
      },
    });
  };
  try {
    const preparationKey = newTaskPreparationKey(state);
    const pendingPreparation = preparationKey ? pendingPreparedNewTask?.(preparationKey) : undefined;
    const preparedTask = pendingPreparation ? (await pendingPreparation).task : undefined;
    let { task, taskId } = await prepareNewTask(
      { backendConnection, dispatch, onPreparedTask: openPreparedTask, state },
      {
        acceptPreparedTask: () => !attempt.cancelled,
        preparedTask,
        snapshotIntent: currentNavigationGeneration() === navigationGeneration ? "open" : "refresh",
      },
    );
    createdTaskId = taskId;
    attempt.taskId = taskId;
    if (attempt.cancelled) {
      await discardOrCancelStartedTask(request, taskId);
      if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
      return;
    }

    task = await waitUntilTaskSendReady(request, task, dispatch);
    taskId = task.task.taskId as TaskId;
    attempt.taskId = taskId;
    if (attempt.cancelled) {
      await discardOrCancelStartedTask(request, taskId);
      if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
      return;
    }
    if (!openedPreparedTask) {
      dispatch({ type: "taskInput:prompt", taskId, prompt: draftInput.prompt });
      dispatch({ type: "taskInput:submit", taskId });
    }
    const idempotencyKey = createTaskSendIdempotencyKey();
    const message = attachments?.length ? { text: draftInput.prompt, attachments } : { text: draftInput.prompt };
    savePendingTaskSendRecovery({
      taskId,
      taskRevision: task.revision,
      idempotencyKey,
      message,
      renderState: draftInput,
    });
    const sent = await sendNewTaskMessageWithFreshRevision({
      dispatch,
      idempotencyKey,
      message,
      request,
      taskId,
      taskRevision: task.revision,
    });
    clearPendingTaskSendRecovery(taskId);
    if (attempt.cancelled) {
      await request(TASK_CANCEL, { taskId });
      if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
      return;
    }
    const snapshot = mapProtocolTaskSnapshot(sent.task).snapshot;
    dispatch({ type: "snapshot", snapshot, intent: "refresh" });
    if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
  } catch (error) {
    if (attempt.cancelled) {
      if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
      return;
    }
    const message = submitErrorMessage(error);
    if (createdTaskId) {
      if (!shouldPreservePendingSendRecovery()) {
        clearPendingTaskSendRecovery(createdTaskId);
      }
      if (isInvalidAttachmentHandleError(error)) {
        dispatch({
          type: "submit:attachments:invalidate",
          taskId: createdTaskId,
          message: error.message,
        });
        if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
        return;
      }
      dispatch({ type: "taskInput:prompt", taskId: createdTaskId, prompt: draftInput.prompt });
      dispatch({ type: "taskInput:error", taskId: createdTaskId, message });
      dispatch({ type: "submit:error", message });
      if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
      return;
    }
    dispatch({ type: "submit:error", message });
    if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
  }
}

export async function prepareNewTask({
  backendConnection,
  dispatch,
  onPreparedTask,
  state,
}: Pick<NewTaskSubmitDependencies, "backendConnection" | "dispatch" | "state"> & { onPreparedTask?: PreparedTaskCallback }, options: PrepareNewTaskOptions = {}) {
  const request = backendConnection?.request;
  if (!request) {
    throw new Error("App Server connection unavailable.");
  }
  const projectId = state.newTask.selection.projectId;
  if (!projectId) {
    throw new Error("Project unavailable. Refresh and try again.");
  }

  const preparedTask = options.preparedTask ?? (state.snapshot && !state.snapshot.task.has_messages
    ? await request(TASK_OPEN, { taskId: state.snapshot.task.task_id as TaskId }).then((result) => result.task)
    : (await request(TASK_CREATE, taskCreateParams(state, projectId))).task);
  if (!preparedTask) throw new Error("Task preparation returned no Task snapshot.");
  let task: ProtocolTaskSnapshot = preparedTask;
  let taskId = task.task.taskId as TaskId;
  if (options.acceptPreparedTask && !options.acceptPreparedTask()) {
    return { task, taskId };
  }
  dispatch({ type: "snapshot", snapshot: mapProtocolTaskSnapshot(task).snapshot, intent: options.snapshotIntent ?? "open" });
  dispatch({ type: "newTask:prepared", taskId });
  onPreparedTask?.(task, taskId);

  if (options.waitForSendReady === false) {
    return { task, taskId };
  }

  task = await waitUntilTaskSendReady(request, task, dispatch);
  taskId = task.task.taskId as TaskId;

  for (const [configId, value] of Object.entries(state.newTask.selection.configOptions)) {
    task = (await request(TASK_SET_CONFIG_OPTION, {
      taskId: task.task.taskId,
      configId: configId as AgentConfigOptionId,
      value,
      clientMutationId: createNewTaskMutationId(configId),
    })).task;
    taskId = task.task.taskId as TaskId;
    dispatch({ type: "snapshot", snapshot: mapProtocolTaskSnapshot(task).snapshot, intent: "refresh" });
  }

  return { task, taskId };
}
