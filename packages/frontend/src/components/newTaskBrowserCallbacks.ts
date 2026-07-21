import {
  ATTACHMENT_CREATE_FILE_REFERENCE,
  ATTACHMENT_LIST_DIRECTORY,
  ATTACHMENT_LIST_ROOTS,
  CLIENT_HEARTBEAT,
  TASK_SEARCH_FILES,
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
import {
  appServerAttachment,
  appServerImageAttachment,
  localImageAttachment,
} from "../state/composerOptions";
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
import { currentFrontendShell } from "../services/frontendShell";

type NewTaskBrowserDependencies = Pick<
  AppCallbacksDependencies,
  | "attachmentResources"
  | "backendConnection"
  | "asyncOperations"
  | "dispatch"
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
  asyncOperations,
  backendConnection,
}: Pick<NewTaskBrowserDependencies, "asyncOperations" | "backendConnection">) {
  const request = backendConnection?.request;
  if (!request) return undefined;
  const operation = asyncOperations.scope("new-task-workspace-browser", "new-task");
  return {
    ownerKey: `new-task-workspace:${operation.id}`,
    listRoots: async () => {
      const result = await request(WORKSPACE_LIST_ROOTS, {});
      if (!asyncOperations.owns(operation)) throw new SupersededNewTaskFileBrowserOperation();
      return result.roots;
    },
    listDirectory: async (path: string) => {
      const result = await request(WORKSPACE_LIST_DIRECTORY, { path });
      if (!asyncOperations.owns(operation)) throw new SupersededNewTaskFileBrowserOperation();
      return result;
    },
  };
}

function createFileBrowserCallbacks({
  attachmentResources,
  asyncOperations,
  backendConnection,
  dispatch,
  pendingPreparedNewTask,
  newTaskController,
  state,
}: NewTaskBrowserDependencies) {
  const request = backendConnection?.request;
  const files = currentFrontendShell()?.files;
  if (!request) return undefined;
  const preparationKey = newTaskPreparationKey(state);
  const operation = asyncOperations.scope(
    "new-task-file-browser",
    preparationKey ?? "unavailable",
  );
  let preparedTaskId = preparedSnapshotMatchesSelection(state)
    ? state.snapshot?.task.task_id as TaskId
    : undefined;
  const operationIsCurrent = () =>
    preparationKey !== undefined
    && asyncOperations.owns(operation);
  const assertOperationCurrent = () => {
    if (!operationIsCurrent()) throw new SupersededNewTaskFileBrowserOperation();
  };
  const discardNewTask = (taskId: TaskId) => newTaskController.discard({
    attachmentResources,
    dispatch,
    lease: newTaskController.currentLease(taskId),
    request,
    taskId,
  });
  const ensureTaskId = async (draft?: NewTaskDraftInput, ignorePendingPreparation = false) => {
    assertOperationCurrent();
    const activePreparationKey = preparationKey as string;
    // A native/web picker can outlive the render that created these callbacks.
    // Reuse preparation that completed while the picker was open instead of
    // acquiring a second Task and releasing the ready options snapshot.
    if (!preparedTaskId) {
      const retainedSnapshot = newTaskController.getSnapshot();
      if (
        retainedSnapshot
        && preparedSnapshotMatchesSelection({ ...state, snapshot: retainedSnapshot })
      ) {
        // A picker can retain callbacks from the render immediately before the
        // controller adopted the matching Prepared Task. Matching product
        // identity is sufficient to reclaim that Task under the current key;
        // discarding it here would release the Task and then upload to its dead ID.
        newTaskController.claim({
          attachmentResources,
          preparationKey: activePreparationKey,
          taskId: retainedSnapshot.task.task_id as TaskId,
        });
        preparedTaskId = retainedSnapshot.task.task_id as TaskId;
      }
    }
    if (preparedTaskId) {
      return newTaskController.claim({
        attachmentResources,
        preparationKey: activePreparationKey,
        taskId: preparedTaskId,
      });
    }

    const staleTaskId = disposableNewTaskControllerId(state, newTaskController);
    const pending = ignorePendingPreparation
      ? undefined
      : pendingPreparedNewTask(activePreparationKey);
    const pendingResult = pending ? await pending : undefined;
    if (pendingResult?.task && !preparedProtocolTaskMatchesSelection(pendingResult.task, state)) {
      await discardNewTask(pendingResult.taskId);
      throw new SupersededNewTaskFileBrowserOperation();
    }
    // The pending preparation may resolve to the same Task held by the
    // controller. Validate it before discarding anything; releasing first makes
    // the subsequent upload target an ID that App Server has already deleted.
    if (staleTaskId && staleTaskId !== pendingResult?.taskId) {
      await discardNewTask(staleTaskId);
    }
    assertOperationCurrent();
    const prepared = await prepareNewTask(
      { backendConnection, dispatch, state },
      {
        acceptPreparedTask: (task) =>
          operationIsCurrent()
          && preparedProtocolTaskMatchesSelection(task, state),
        discardPreparedTask: discardNewTask,
        preparedTask: pendingResult?.task,
        reuseSnapshot: false,
      },
    );
    if (
      !operationIsCurrent()
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
    return lease;
  };
  const releaseLateHandle = (taskId: TaskId, handleId: AttachmentHandleId) => {
    if (attachmentResources) {
      attachmentResources.release({ taskId, handleId });
      return;
    }
    releaseAttachmentResources(backendConnection, taskId, [attachmentHandleResource(handleId)]);
  };

  return {
    ownerKey: `new-task-files:${operation.id}`,
    ...(files ? { attachmentMode: files.kind, attachFiles: async (
      selectedFiles: File[],
      options: { onProgress: (progress: { loaded: number; total: number }) => void; signal: AbortSignal; maxFiles: number },
    ) => {
      const pickerLease = newTaskController.currentLease(preparedTaskId);
      // A native file picker can suspend browser timers past the App Server's
      // client-liveness window. Probe first so reliable transport recovery
      // completes before choosing the Task that will own the upload.
      await request(CLIENT_HEARTBEAT, {});
      const clientLeaseExpired = Boolean(
        pickerLease && !newTaskController.isCurrent(pickerLease),
      );
      if (clientLeaseExpired) preparedTaskId = undefined;
      const lease = await ensureTaskId(undefined, clientLeaseExpired);
      const attachments = files.kind === "nativePicker"
        ? await files.pick(lease.taskId)
        : selectedFiles.length === 1
          ? [await files.upload(lease.taskId, selectedFiles[0], options.onProgress, options.signal)]
          : [];
      // After Task acquisition, the controller lease—not a render-scoped callback
      // token—owns the upload. Harmless New Task rerenders can replace callback
      // scopes while the same Prepared Task remains authoritative.
      if (!newTaskController.isCurrent(lease)) {
        attachments.forEach((attachment) => releaseLateHandle(lease.taskId, attachment.handleId));
        throw new SupersededNewTaskFileBrowserOperation();
      }
      const adoption = attachmentResources?.beginAdoption(lease.taskId);
      if (attachmentResources && !adoption) {
        attachments.forEach((attachment) => releaseLateHandle(lease.taskId, attachment.handleId));
        throw new SupersededNewTaskFileBrowserOperation();
      }
      for (const attachment of attachments.slice(0, options.maxFiles)) {
        if (attachmentResources?.adopt({ taskId: lease.taskId, handleId: attachment.handleId }, adoption) === false) {
          throw new SupersededNewTaskFileBrowserOperation();
        }
        dispatch({
          type: "taskInput:attachment:addAppServer",
          taskId: lease.taskId,
          attachment: appServerAttachment(attachment),
        });
      }
      for (const attachment of attachments.slice(options.maxFiles)) {
        releaseLateHandle(lease.taskId, attachment.handleId);
      }
    } } : {}),
    searchFiles: async (query: string) => {
      const lease = await ensureTaskId();
      const result = await request(TASK_SEARCH_FILES, { taskId: lease.taskId, query });
      assertOperationCurrent();
      return result;
    },
    listRoots: async () => {
      const lease = await ensureTaskId();
      const result = await request(ATTACHMENT_LIST_ROOTS, { taskId: lease.taskId });
      assertOperationCurrent();
      return result.roots;
    },
    listDirectory: async (rootId: FileBrowserRootId, directoryId?: FileBrowserEntryId) => {
      const lease = await ensureTaskId();
      const result = await request(ATTACHMENT_LIST_DIRECTORY, {
        taskId: lease.taskId,
        rootId,
        directoryId,
      });
      assertOperationCurrent();
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
      if (!operationIsCurrent()) {
        releaseLateHandle(lease.taskId, result.attachment.handleId);
        throw new SupersededNewTaskFileBrowserOperation();
      }
      dispatch({
        type: "taskInput:attachment:addAppServer",
        taskId: lease.taskId,
        attachment: appServerAttachment(result.attachment),
      });
    },
    attachImage: async (file: File, draft?: NewTaskDraftInput) => {
      if (files?.kind === "webUpload") {
        const lease = await ensureTaskId(draft);
        const attachment = await files.upload(
          lease.taskId,
          file,
          () => undefined,
          new AbortController().signal,
          { kind: "image", mimeType: file.type || "image/png" },
        );
        try {
          const data = await fileToBase64(file);
          if (!newTaskController.isCurrent(lease)) {
            throw new SupersededNewTaskFileBrowserOperation();
          }
          const adoption = attachmentResources?.beginAdoption(lease.taskId);
          if (attachmentResources && !adoption) {
            throw new SupersededNewTaskFileBrowserOperation();
          }
          if (attachmentResources?.adopt({ taskId: lease.taskId, handleId: attachment.handleId }, adoption) === false) {
            throw new SupersededNewTaskFileBrowserOperation();
          }
          dispatch({
            type: "newTask:attachment:add",
            attachment: appServerImageAttachment(
              attachment,
              `data:${file.type || "image/png"};base64,${data}`,
            ),
          });
        } catch (error) {
          releaseLateHandle(lease.taskId, attachment.handleId);
          throw error;
        }
        return;
      }
      const data = await fileToBase64(file);
      dispatch({
        type: "newTask:attachment:add",
        attachment: localImageAttachment(file, data),
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
          return operationIsCurrent() ? "current" : "release";
        },
      );
      if (attachmentResources?.adopt({ taskId: lease.taskId, handleId: attachment.handleId }, adoption) === false) {
        throw new SupersededNewTaskFileBrowserOperation();
      }
      if (!operationIsCurrent()) {
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
