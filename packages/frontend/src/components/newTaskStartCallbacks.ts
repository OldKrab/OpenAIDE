import {
  TASK_CANCEL,
  type TaskId,
} from "@openaide/app-server-client";
import { createTaskSendIdempotencyKey } from "../intents/taskMutationIntents";
import {
  releaseComposerAttachments,
} from "../services/attachmentResources";
import { postHostMessage } from "../services/hostBridge";
import { clearPendingTaskSendRecovery } from "../services/pendingTaskSendRecovery";
import {
  executeTaskSendAttempt,
  isTaskSendOutcomeUnknown,
  resolveTaskSendAttempt,
  TASK_SEND_OUTCOME_UNKNOWN_MESSAGE,
  taskSendAttemptRecord,
} from "../services/taskSendAttempt";
import { isInvalidAttachmentHandleError } from "../state/attachmentValidation";
import { appServerAttachmentHandles } from "../state/composerOptions";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import {
  newTaskPreparationKey,
  preparedTaskMatchesNewTaskContext,
} from "../state/newTaskPreparationContext";
import type {
  AppCallbacksDependencies,
  NewTaskCallbacks,
  NewTaskDraftInput,
  NewTaskStartAttempt,
} from "./appControllerCallbackTypes";
import {
  prepareNewTask,
} from "./newTaskPreparation";
import {
  discardOrCancelStartedTask,
  newTaskDraftInput,
  submitErrorMessage,
} from "./newTaskStartSupport";
import type { NewTaskController, NewTaskLease } from "./newTaskController";

type NewTaskStartDependencies = Pick<
  AppCallbacksDependencies,
  | "attachmentResources"
  | "backendConnection"
  | "beginNavigationChange"
  | "clientInstanceId"
  | "currentNavigationGeneration"
  | "dispatch"
  | "newTaskStartAttempt"
  | "pendingPreparedNewTask"
  | "state"
> & { newTaskController: NewTaskController };

/** Owns the complete first-send lifecycle, including cancellation and ambiguous outcomes. */
export function createNewTaskStartCallbacks(
  dependencies: NewTaskStartDependencies,
): Pick<NewTaskCallbacks, "cancel" | "submit"> {
  return {
    cancel: () => cancelNewTaskStart(dependencies),
    submit: (draft) => {
      void submitNewTask({ ...dependencies, draft });
    },
  };
}

function cancelNewTaskStart({
  attachmentResources,
  backendConnection,
  beginNavigationChange,
  clientInstanceId,
  dispatch,
  newTaskStartAttempt,
  newTaskController,
  state,
}: NewTaskStartDependencies) {
  const attempt = newTaskStartAttempt.current;
  const nativeSessionOpening = state.newTask.nativeSessions.adoptingSessionId !== undefined;
  if ((!attempt && !nativeSessionOpening) || attempt?.cancelled) return;
  beginNavigationChange();
  if (attempt) attempt.cancelled = true;
  dispatch({ type: "submit:cancel" });
  postHostMessage(state.newTask.selection.projectId
    ? { type: "surface.openNewTask", payload: { project_id: state.newTask.selection.projectId } }
    : { type: "surface.openNewTask" });
  if (!attempt || attempt.sendInFlight) return;
  if (attempt.taskId && backendConnection?.request) {
    const taskId = attempt.taskId;
    const preparationKey = newTaskPreparationKey(state);
    const lease = attempt.newTaskLease
      ?? newTaskController.currentLease(taskId)
      ?? (preparationKey ? newTaskController.claim({ attachmentResources, preparationKey, taskId }) : undefined);
    const protectionKey = lease ? `prepared-cancel:${lease.generation}` : undefined;
    if (lease && protectionKey) newTaskController.protectSend(lease, protectionKey);
    void discardOrCancelStartedTask(backendConnection.request, taskId).then((outcome) => {
      if (outcome !== "discarded" || !attempt.taskId) return;
      newTaskController.recordDiscarded(attempt.taskId);
      if (state.appServerStateRootId) {
        clearPendingTaskSendRecovery(state.appServerStateRootId, clientInstanceId, attempt.taskId);
      }
      releaseComposerAttachments({
        attachmentResources,
        attachments: attempt.draft.context,
        backendConnection,
        taskId: attempt.taskId,
      });
      dispatch({ type: "taskInput:clear", taskId: attempt.taskId });
    }).catch((error) => {
      if (protectionKey) newTaskController.settleSend(protectionKey);
      if (lease) newTaskController.reclaim(lease, attachmentResources);
      dispatch({ type: "submit:error", message: submitErrorMessage(error) });
    }).finally(() => {
      if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
    });
  }
}

type SubmitNewTaskDependencies = NewTaskStartDependencies & { draft?: NewTaskDraftInput };

async function submitNewTask({
  attachmentResources,
  backendConnection,
  clientInstanceId,
  currentNavigationGeneration,
  dispatch,
  draft,
  newTaskStartAttempt,
  pendingPreparedNewTask,
  newTaskController,
  state,
}: SubmitNewTaskDependencies) {
  const request = backendConnection?.request;
  if (!request) {
    dispatch({ type: "submit:error", message: "App Server connection unavailable." });
    return;
  }
  const stateRootId = state.appServerStateRootId;
  if (!stateRootId) {
    dispatch({ type: "submit:error", message: "App Server state root unavailable. Refresh and try again." });
    return;
  }
  const projectId = state.newTask.selection.projectId;
  if (!projectId) {
    dispatch({ type: "submit:error", message: "Workspace unavailable. Refresh and try again." });
    return;
  }
  const preparationKey = newTaskPreparationKey(state);
  if (!preparationKey) {
    dispatch({ type: "submit:error", message: "Task context is not ready yet." });
    return;
  }
  const draftInput = newTaskDraftInput(state, draft);
  const attachments = appServerAttachmentHandles(draftInput.context);
  if (draftInput.context.length > 0 && !attachments) {
    dispatch({ type: "submit:error", message: "Reselect attachments from the file browser before sending." });
    return;
  }

  const proposedIdempotencyKey = createTaskSendIdempotencyKey();
  const attempt: NewTaskStartAttempt = { cancelled: false, draft: draftInput };
  newTaskStartAttempt.current = attempt;
  attachmentResources?.lockAdoptions();
  dispatch(draft
    ? { type: "submit:start", prompt: draftInput.prompt, context: draftInput.context, idempotencyKey: proposedIdempotencyKey }
    : { type: "submit:start", idempotencyKey: proposedIdempotencyKey });
  const navigationGeneration = currentNavigationGeneration();
  let createdTaskId: TaskId | undefined;
  let newTaskLease: NewTaskLease | undefined;
  let sendAttempt: ReturnType<typeof taskSendAttemptRecord> | undefined;
  const discardNewTask = (taskId: TaskId) => newTaskController.discard({
    attachmentResources,
    dispatch,
    lease: newTaskController.currentLease(taskId),
    request,
    taskId,
  });
  const settleDiscardedTask = (taskId: TaskId) => {
    newTaskController.recordDiscarded(taskId);
    clearPendingTaskSendRecovery(stateRootId, clientInstanceId, taskId);
    releaseComposerAttachments({
      attachmentResources,
      attachments: draftInput.context,
      backendConnection,
      taskId,
    });
    dispatch({ type: "taskInput:clear", taskId });
  };
  const stagePreparedTask = (_task: unknown, taskId: TaskId) => {
    dispatch({ type: "taskInput:prompt", taskId, prompt: draftInput.prompt });
  };
  const discardOrCancelOwnedTask = async (taskId: TaskId, lease: NonNullable<typeof newTaskLease>) => {
    const protectionKey = `prepared-cancel:${lease.generation}`;
    newTaskController.protectSend(lease, protectionKey);
    try {
      const outcome = await discardOrCancelStartedTask(request, taskId);
      if (outcome === "discarded") newTaskController.settleSend(protectionKey);
      return outcome;
    } catch (error) {
      newTaskController.settleSend(protectionKey);
      newTaskController.reclaim(lease, attachmentResources);
      throw error;
    }
  };
  try {
    const cachedSnapshot = newTaskController.getSnapshot();
    const cachedNewTask = cachedSnapshot?.lifecycle === "new"
      && preparedTaskMatchesNewTaskContext(state, {
        agentId: cachedSnapshot.task.agent_id,
        projectId: cachedSnapshot.task.project_id,
        workspaceRoot: cachedSnapshot.task.workspace_root,
      })
      ? cachedSnapshot
      : undefined;
    let taskId: TaskId;
    let taskRevision: number;
    let taskTitle: string;
    if (cachedNewTask) {
      taskId = cachedNewTask.task.task_id as TaskId;
      taskRevision = cachedNewTask.revision;
      taskTitle = cachedNewTask.task.title ?? "New task";
      newTaskLease = newTaskController.currentLease(taskId) ?? newTaskController.claim({
        attachmentResources,
        preparationKey,
        taskId,
      });
    } else {
      const pendingPreparation = pendingPreparedNewTask(preparationKey);
      const pendingTask = pendingPreparation ? (await pendingPreparation).task : undefined;
      const prepared = await prepareNewTask(
        { backendConnection, dispatch, onPreparedTask: stagePreparedTask, state },
        {
          acceptPreparedTask: () => !attempt.cancelled,
          discardPreparedTask: discardNewTask,
          preparedTask: pendingTask,
          snapshotIntent: currentNavigationGeneration() === navigationGeneration ? "open" : "refresh",
        },
      );
      taskId = prepared.taskId;
      taskRevision = prepared.task.revision;
      taskTitle = prepared.task.task.title?.value
        ?? (prepared.task.lifecycle === "new" ? "New task" : "Untitled task");
      const snapshot = mapProtocolTaskSnapshot(prepared.task).snapshot;
      newTaskLease = newTaskController.retain({
        attachmentResources,
        preparationKey,
        snapshot,
      });
      if (!newTaskLease) throw new Error("New Task changed before Send could start.");
    }
    createdTaskId = taskId;
    attempt.taskId = taskId;
    attempt.newTaskLease = newTaskLease;
    if (attempt.cancelled) {
      try {
        const outcome = await discardOrCancelOwnedTask(taskId, newTaskLease);
        if (outcome === "discarded") {
          settleDiscardedTask(taskId);
        }
      } catch (cleanupError) {
        dispatch({ type: "submit:error", message: submitErrorMessage(cleanupError) });
      }
      if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
      return;
    }

    const message = attachments?.length ? { text: draftInput.prompt, attachments } : { text: draftInput.prompt };
    sendAttempt = resolveTaskSendAttempt(taskSendAttemptRecord({
      clientInstanceId,
      idempotencyKey: proposedIdempotencyKey,
      message,
      renderState: draftInput,
      stateRootId,
      taskId,
      taskRevision,
    }));
    dispatch({ type: "submit:attempt", idempotencyKey: sendAttempt.idempotencyKey });
    dispatch({
      type: "taskInput:submit",
      taskId,
      input: draftInput,
      idempotencyKey: sendAttempt.idempotencyKey,
    });
    attempt.sendInFlight = true;
    const pendingSend = executeTaskSendAttempt({
      attempt: sendAttempt,
      backendConnection: { request },
      refreshRevisionOnConflict: true,
    });
    // A durable send receipt, rather than a route render, transfers ownership to Task Chat.
    newTaskController.protectSend(newTaskLease, sendAttempt.idempotencyKey);
    if (currentNavigationGeneration() === navigationGeneration) {
      postHostMessage({
        type: "surface.openTask",
        payload: {
          task_id: taskId,
          title: taskTitle,
        },
      });
    }
    const { attempt: acceptedAttempt, result: sent } = await pendingSend;
    attempt.sendInFlight = false;
    newTaskController.settleSend(acceptedAttempt.idempotencyKey);
    newTaskController.confirmSentTask(taskId);
    const snapshot = mapProtocolTaskSnapshot(sent.task).snapshot;
    dispatch({
      type: "task:promoted",
      snapshot,
      activate: currentNavigationGeneration() === navigationGeneration,
    });
    dispatch({
      type: "taskSend:accepted",
      taskId,
      idempotencyKey: acceptedAttempt.idempotencyKey,
      userMessageId: sent.userMessageId,
    });
    if (attempt.cancelled) {
      await request(TASK_CANCEL, { taskId });
      if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
      return;
    }
    if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
  } catch (error) {
    attempt.sendInFlight = false;
    if (attempt.cancelled) {
      if (createdTaskId) {
        try {
          const lease = newTaskLease ?? attempt.newTaskLease;
          if (!lease) throw new Error("New Task lease unavailable during cancellation.");
          const outcome = await discardOrCancelOwnedTask(createdTaskId, lease);
          if (outcome === "discarded") {
            settleDiscardedTask(createdTaskId);
          }
        } catch (cleanupError) {
          dispatch({ type: "submit:error", message: submitErrorMessage(cleanupError) });
        }
      }
      if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
      return;
    }
    const message = submitErrorMessage(error);
    if (createdTaskId) {
      if (isInvalidAttachmentHandleError(error)) {
        if (sendAttempt) newTaskController.settleSend(sendAttempt.idempotencyKey);
        if (newTaskLease) newTaskController.reclaim(newTaskLease, attachmentResources);
        releaseComposerAttachments({
          attachmentResources,
          attachments: draftInput.context,
          backendConnection,
          taskId: createdTaskId,
        });
        dispatch({
          type: "submit:attachments:invalidate",
          taskId: createdTaskId,
          message: error.message,
        });
        if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
        return;
      }
      if (isTaskSendOutcomeUnknown(error)) {
        dispatch({
          type: "taskInput:sendUncertain",
          taskId: createdTaskId,
          idempotencyKey: sendAttempt?.idempotencyKey ?? proposedIdempotencyKey,
          message: TASK_SEND_OUTCOME_UNKNOWN_MESSAGE,
        });
      } else if (sendAttempt) {
        newTaskController.settleSend(sendAttempt.idempotencyKey);
        if (newTaskLease) newTaskController.reclaim(newTaskLease, attachmentResources);
        dispatch({
          type: "taskInput:sendError",
          taskId: createdTaskId,
          idempotencyKey: sendAttempt.idempotencyKey,
          message,
        });
      } else {
        dispatch({ type: "taskInput:error", taskId: createdTaskId, message });
      }
      dispatch({ type: "submit:error", message });
      if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
      return;
    }
    dispatch({ type: "submit:error", message });
    if (newTaskStartAttempt.current === attempt) newTaskStartAttempt.current = undefined;
  }
}
