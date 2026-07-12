import {
  ATTACHMENT_CREATE_FILE_REFERENCE,
  ATTACHMENT_CREATE_PASTED_IMAGE,
  ATTACHMENT_LIST_DIRECTORY,
  ATTACHMENT_LIST_ROOTS,
  WORKSPACE_LIST_DIRECTORY,
  WORKSPACE_LIST_ROOTS,
  type AttachmentHandleId,
  type FileBrowserEntryId,
  type FileBrowserRootId,
  type TaskId,
} from "@openaide/app-server-client";
import {
  attachmentHandleResource,
  releaseAttachmentResources,
} from "../services/attachmentResources";
import { createConfirmedEmbeddedAttachment } from "../services/embeddedAttachmentSelection";
import { appServerAttachment } from "../state/composerOptions";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import { newTaskPreparationKey } from "../state/newTaskPreparationContext";
import type {
  AppCallbacksDependencies,
  NewTaskCallbacks,
  NewTaskDraftInput,
} from "./appControllerCallbackTypes";
import {
  prepareNewTask,
  preparedProtocolTaskMatchesSelection,
  preparedSnapshotMatchesSelection,
} from "./newTaskPreparation";
import {
  disposableNewTaskControllerId,
  type NewTaskController,
} from "./newTaskController";

type NewTaskBrowserDependencies = Pick<
  AppCallbacksDependencies,
  | "attachmentResources"
  | "backendConnection"
  | "currentNavigationGeneration"
  | "currentNewTaskPreparationKey"
  | "dispatch"
  | "latestOptionsRequestKey"
  | "pendingPreparedNewTask"
  | "state"
> & { newTaskController: NewTaskController };

/** Builds both browser surfaces while hiding prepared-Task leases and late-result cleanup. */
export function createNewTaskBrowserCallbacks(
  dependencies: NewTaskBrowserDependencies,
): Pick<NewTaskCallbacks, "fileBrowser" | "workspaceBrowser"> {
  return {
    fileBrowser: createFileBrowserCallbacks(dependencies),
    workspaceBrowser: createWorkspaceBrowserCallbacks(dependencies),
  };
}

function createWorkspaceBrowserCallbacks({
  backendConnection,
  currentNavigationGeneration,
}: Pick<NewTaskBrowserDependencies, "backendConnection" | "currentNavigationGeneration">) {
  const request = backendConnection?.request;
  if (!request) return undefined;
  return {
    ownerKey: `new-task-workspace:${currentNavigationGeneration()}`,
    listRoots: async () => (await request(WORKSPACE_LIST_ROOTS, {})).roots,
    listDirectory: async (path: string) => request(WORKSPACE_LIST_DIRECTORY, { path }),
  };
}

function createFileBrowserCallbacks({
  attachmentResources,
  backendConnection,
  currentNavigationGeneration,
  currentNewTaskPreparationKey,
  dispatch,
  latestOptionsRequestKey,
  pendingPreparedNewTask,
  newTaskController,
  state,
}: NewTaskBrowserDependencies) {
  const request = backendConnection?.request;
  if (!request) return undefined;
  const preparationKey = newTaskPreparationKey(state);
  let preparedTaskId = preparedSnapshotMatchesSelection(state)
    ? state.snapshot?.task.task_id as TaskId
    : undefined;
  const operationIsCurrent = (navigationGeneration: number) =>
    preparationKey !== undefined
    && currentNewTaskPreparationKey() === preparationKey
    && currentNavigationGeneration() === navigationGeneration;
  const assertOperationCurrent = (navigationGeneration: number) => {
    if (!operationIsCurrent(navigationGeneration)) throw new SupersededNewTaskFileBrowserOperation();
  };
  const discardNewTask = (taskId: TaskId) => newTaskController.discard({
    attachmentResources,
    dispatch,
    lease: newTaskController.currentLease(taskId),
    request,
    taskId,
  });
  const ensureTaskId = async (draft?: NewTaskDraftInput) => {
    const navigationGeneration = currentNavigationGeneration();
    assertOperationCurrent(navigationGeneration);
    const activePreparationKey = preparationKey as string;
    if (preparedTaskId) {
      newTaskController.claim({
        attachmentResources,
        preparationKey: activePreparationKey,
        taskId: preparedTaskId,
      });
      return { navigationGeneration, taskId: preparedTaskId };
    }

    const staleTaskId = disposableNewTaskControllerId(state, newTaskController);
    if (staleTaskId) await discardNewTask(staleTaskId);
    assertOperationCurrent(navigationGeneration);
    latestOptionsRequestKey.current = undefined;
    const pending = pendingPreparedNewTask(activePreparationKey);
    const pendingResult = pending ? await pending : undefined;
    if (pendingResult?.task && !preparedProtocolTaskMatchesSelection(pendingResult.task, state)) {
      await discardNewTask(pendingResult.taskId);
      throw new SupersededNewTaskFileBrowserOperation();
    }
    const prepared = await prepareNewTask(
      { backendConnection, dispatch, state },
      {
        acceptPreparedTask: (task) =>
          operationIsCurrent(navigationGeneration)
          && preparedProtocolTaskMatchesSelection(task, state),
        discardPreparedTask: discardNewTask,
        preparedTask: pendingResult?.task,
        reuseSnapshot: false,
      },
    );
    if (
      !operationIsCurrent(navigationGeneration)
      || !preparedProtocolTaskMatchesSelection(prepared.task, state)
    ) {
      await discardNewTask(prepared.taskId);
      throw new SupersededNewTaskFileBrowserOperation();
    }
    preparedTaskId = prepared.taskId;
    const lease = newTaskController.retain({
      attachmentResources,
      preparationKey: activePreparationKey,
      snapshot: mapProtocolTaskSnapshot(prepared.task).snapshot,
    });
    if (!lease) {
      await discardNewTask(prepared.taskId);
      throw new SupersededNewTaskFileBrowserOperation();
    }
    // `newTask:prepared` transfers current reducer text. An explicit paste draft
    // can be newer than that state transition and is the only required override.
    if (draft) {
      dispatch({ type: "taskInput:prompt", taskId: prepared.taskId, prompt: draft.prompt });
    }
    return { navigationGeneration, taskId: preparedTaskId };
  };
  const releaseLateHandle = (taskId: TaskId, handleId: AttachmentHandleId) => {
    if (attachmentResources) {
      attachmentResources.release({ taskId, handleId });
      return;
    }
    releaseAttachmentResources(backendConnection, taskId, [attachmentHandleResource(handleId)]);
  };

  return {
    ownerKey: `new-task-files:${currentNavigationGeneration()}:${preparationKey ?? "unavailable"}`,
    listRoots: async () => {
      const lease = await ensureTaskId();
      const result = await request(ATTACHMENT_LIST_ROOTS, { taskId: lease.taskId });
      assertOperationCurrent(lease.navigationGeneration);
      return result.roots;
    },
    listDirectory: async (rootId: FileBrowserRootId, directoryId?: FileBrowserEntryId) => {
      const lease = await ensureTaskId();
      const result = await request(ATTACHMENT_LIST_DIRECTORY, {
        taskId: lease.taskId,
        rootId,
        directoryId,
      });
      assertOperationCurrent(lease.navigationGeneration);
      return result;
    },
    attachFileReference: async (entryId: FileBrowserEntryId) => {
      const lease = await ensureTaskId();
      const adoption = attachmentResources?.beginAdoption(lease.taskId);
      if (attachmentResources && !adoption) throw new SupersededNewTaskFileBrowserOperation();
      const result = await request(ATTACHMENT_CREATE_FILE_REFERENCE, {
        taskId: lease.taskId,
        entryId,
      });
      if (attachmentResources?.adopt({ taskId: lease.taskId, handleId: result.attachment.handleId }, adoption) === false) {
        throw new SupersededNewTaskFileBrowserOperation();
      }
      if (!operationIsCurrent(lease.navigationGeneration)) {
        releaseLateHandle(lease.taskId, result.attachment.handleId);
        throw new SupersededNewTaskFileBrowserOperation();
      }
      dispatch({
        type: "taskInput:attachment:addAppServer",
        taskId: lease.taskId,
        attachment: appServerAttachment(result.attachment),
      });
    },
    attachPastedImage: async (file: File, draft?: NewTaskDraftInput) => {
      const lease = await ensureTaskId(draft);
      const adoption = attachmentResources?.beginAdoption(lease.taskId);
      if (attachmentResources && !adoption) throw new SupersededNewTaskFileBrowserOperation();
      const data = await fileToBase64(file);
      const previewUrl = `data:${file.type || "image/png"};base64,${data}`;
      const result = await request(ATTACHMENT_CREATE_PASTED_IMAGE, {
        taskId: lease.taskId,
        label: file.name || "Pasted image",
        mimeType: file.type || "image/png",
        data,
      });
      if (attachmentResources?.adopt({ taskId: lease.taskId, handleId: result.attachment.handleId }, adoption) === false) {
        throw new SupersededNewTaskFileBrowserOperation();
      }
      if (!operationIsCurrent(lease.navigationGeneration)) {
        releaseLateHandle(lease.taskId, result.attachment.handleId);
        throw new SupersededNewTaskFileBrowserOperation();
      }
      dispatch({
        type: "taskInput:attachment:addAppServer",
        taskId: lease.taskId,
        attachment: appServerAttachment(result.attachment, { previewUrl }),
      });
    },
    attachEmbedded: async (entryId: FileBrowserEntryId) => {
      const lease = await ensureTaskId();
      const adoption = attachmentResources?.beginAdoption(lease.taskId);
      if (attachmentResources && !adoption) throw new SupersededNewTaskFileBrowserOperation();
      const attachment = await createConfirmedEmbeddedAttachment(
        { request },
        lease.taskId,
        entryId,
        () => {
          if (attachmentResources && adoption) {
            const status = attachmentResources.adoptionStatus(adoption);
            if (status === "replacedReplica") return "forget";
            if (status === "expired") return "release";
          }
          return operationIsCurrent(lease.navigationGeneration) ? "current" : "release";
        },
      );
      if (attachmentResources?.adopt({ taskId: lease.taskId, handleId: attachment.handleId }, adoption) === false) {
        throw new SupersededNewTaskFileBrowserOperation();
      }
      if (!operationIsCurrent(lease.navigationGeneration)) {
        releaseLateHandle(lease.taskId, attachment.handleId);
        throw new SupersededNewTaskFileBrowserOperation();
      }
      dispatch({
        type: "taskInput:attachment:addAppServer",
        taskId: lease.taskId,
        attachment: appServerAttachment(attachment),
      });
    },
  };
}

class SupersededNewTaskFileBrowserOperation extends Error {
  constructor() {
    super("New Task context changed before the file selection completed.");
    this.name = "SupersededNewTaskFileBrowserOperation";
  }
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
