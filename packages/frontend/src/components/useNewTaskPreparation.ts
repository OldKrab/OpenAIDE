import { useEffect, useRef, type Dispatch, type MutableRefObject } from "react";
import {
  TASK_ACQUIRE,
  TASK_ACQUIRE_IN_WORKTREE,
  TASK_RELEASE,
  type TaskId,
} from "@openaide/app-server-client";
import type { AppAction } from "../state/appReducer";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import type { AppState } from "../state/store";
import type { ComposerAttachmentResourceOwner } from "../services/attachmentResources";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import type { AppControllerBackendConnection } from "./appControllerBackendLifecycle";
import type { PendingNewTaskPreparationResult } from "./appControllerCallbackTypes";
import type { NewTaskStartAttempt } from "./appControllerCallbackTypes";
import {
  newTaskPreparationKey,
  taskAcquireInWorktreeParams,
  taskAcquireParams,
} from "../state/newTaskPreparationContext";
import type { NewTaskController } from "./newTaskController";
import type { AsyncOperationOwner } from "../state/asyncOperationOwner";
import {
  readRetainedPreparedTaskLease,
  retainPreparedTaskLease,
} from "../state/newTaskSelectionDefaults";

export type PendingNewTaskPreparation = {
  key: string;
  promise: Promise<PendingNewTaskPreparationResult>;
  settled: boolean;
};

type NewTaskPreparationOptions = {
  backendConnection?: AppControllerBackendConnection;
  backendReady: boolean;
  bootstrap: WebviewBootstrap;
  clientInstanceId: string;
  attachmentResources?: ComposerAttachmentResourceOwner;
  asyncOperations: AsyncOperationOwner;
  dispatch: Dispatch<AppAction>;
  pendingPreparation: MutableRefObject<PendingNewTaskPreparation | undefined>;
  newTaskController: NewTaskController;
  replicaEpoch: number;
  startAttempt: MutableRefObject<NewTaskStartAttempt | undefined>;
  state: AppState;
};

/** Starts the Task/session boundary once the required new-task context exists. */
export function useNewTaskPreparation({
  attachmentResources,
  asyncOperations,
  backendConnection,
  backendReady,
  bootstrap,
  clientInstanceId,
  dispatch,
  pendingPreparation,
  newTaskController,
  replicaEpoch,
  startAttempt,
  state,
}: NewTaskPreparationOptions) {
  const preparationKey = newTaskPreparationKey(state);
  const operation = asyncOperations.scope(
    "new-task-preparation",
    preparationKey ?? "unavailable",
    "replica",
  );
  const completedPreparationKey = useRef<string | undefined>(undefined);
  const failedPreparationKey = useRef<string | undefined>(undefined);
  const currentOperationId = useRef(operation.id);
  if (currentOperationId.current !== operation.id) {
    currentOperationId.current = operation.id;
    // Keep an older acquire promise visible so a replacement chains behind its
    // server-side lease instead of issuing a conflicting acquire in parallel.
    completedPreparationKey.current = undefined;
    failedPreparationKey.current = undefined;
  }
  const isNewTaskRoute = bootstrap.surface === "task" && !bootstrap.taskId;
  const previousBootstrap = useRef(bootstrap);
  const enteredNewTaskRoute = isNewTaskRoute && previousBootstrap.current !== bootstrap;
  previousBootstrap.current = bootstrap;
  if (!isNewTaskRoute) completedPreparationKey.current = undefined;
  if (enteredNewTaskRoute) {
    // A completed preparation from the previous route must not mask a fresh
    // acquire. Keep only an in-flight promise so route changes remain ordered.
    if (pendingPreparation.current?.settled) pendingPreparation.current = undefined;
    completedPreparationKey.current = undefined;
  }
  const retainedSnapshot = newTaskController.getSnapshot();
  const replacementTaskId = newTaskController.taskRequiringReplacement();
  const preparationResetKey = newTaskController.preparationResetKey();
  const previousPreparationResetKey = useRef(preparationResetKey);
  const preparationWasReset = previousPreparationResetKey.current !== preparationResetKey;
  previousPreparationResetKey.current = preparationResetKey;
  const preparedTaskMatches = Boolean(
    retainedSnapshot
      && !replacementTaskId
      && retainedSnapshot.lifecycle === "prepared"
      && retainedSnapshot.task.project_id === state.newTask.selection.projectId
      && retainedSnapshot.task.agent_id === state.newTask.selection.agentId
      && retainedSnapshot.task.worktree_id === state.newTask.selection.worktreeId,
  );
  if (replacementTaskId && preparationKey) {
    completedPreparationKey.current = undefined;
    failedPreparationKey.current = undefined;
    // A render caused by local pending feedback must not clear the replacement
    // acquire that produced it. Only discard the settled predecessor here.
    if (pendingPreparation.current?.key === preparationKey && pendingPreparation.current.settled) {
      pendingPreparation.current = undefined;
    }
  }
  if (isNewTaskRoute && preparedTaskMatches && preparationKey) {
    completedPreparationKey.current = preparationKey;
  } else if (preparationWasReset) {
    completedPreparationKey.current = undefined;
    if (pendingPreparation.current?.key === preparationKey) pendingPreparation.current = undefined;
  }

  useEffect(() => {
    if (
      bootstrap.surface !== "task"
      || bootstrap.taskId
      || !backendReady
      || !backendConnection?.request
      || !preparationKey
      || preparedTaskMatches
      || completedPreparationKey.current === preparationKey
      || failedPreparationKey.current === preparationKey
      || pendingPreparation.current?.key === preparationKey
    ) {
      return;
    }

    const request = backendConnection.request;
    const previousPreparation = pendingPreparation.current?.promise;
    const retainedLease = state.appServerStateRootId
      ? readRetainedPreparedTaskLease(state.appServerStateRootId, clientInstanceId)
      : undefined;
    const staleTaskId = retainedSnapshot && !preparedTaskMatches
      ? retainedSnapshot.task.task_id as TaskId
      : undefined;
    const discard = (taskId: TaskId) => newTaskController.discard({
      attachmentResources,
      dispatch,
      lease: newTaskController.currentLease(taskId),
      request,
      taskId,
    });
    const promise = (previousPreparation
      ? previousPreparation.catch(() => undefined)
      : Promise.resolve()
    ).then(async () => {
      if (!asyncOperations.owns(operation)) {
        throw new SupersededPreparation();
      }
      if (staleTaskId && staleTaskId !== replacementTaskId) await discard(staleTaskId);
      if (!staleTaskId && retainedLease && retainedLease.preparationKey !== preparationKey) {
        // Reload can preserve the stable client and selected context while losing the
        // in-memory snapshot. Release the retained lease before acquiring another key.
        await request(TASK_RELEASE, { taskId: retainedLease.taskId as TaskId });
      }
      if (!asyncOperations.owns(operation)) {
        throw new SupersededPreparation();
      }

      const projectId = state.newTask.selection.projectId;
      if (!projectId) throw new SupersededPreparation();
      const task = state.newTask.selection.worktreeId
        ? (await request(TASK_ACQUIRE_IN_WORKTREE, taskAcquireInWorktreeParams(state, projectId))).task
        : (await request(TASK_ACQUIRE, taskAcquireParams(state, projectId))).task;
      const taskId = task.task.taskId as TaskId;
      if (!asyncOperations.owns(operation)) {
        // The server may have leased this Prepared Task before the response
        // became stale locally; release that exact identity before continuing.
        await discard(taskId);
        throw new SupersededPreparation();
      }
      if (state.appServerStateRootId) {
        retainPreparedTaskLease(state.appServerStateRootId, clientInstanceId, {
          preparationKey,
          taskId,
        });
      }
      const cancelledAttempt = startAttempt.current?.cancelled ? startAttempt.current : undefined;
      if (cancelledAttempt) {
        cancelledAttempt.taskId = taskId;
        await discard(taskId);
        if (startAttempt.current === cancelledAttempt) startAttempt.current = undefined;
        return { taskId, task };
      }
      if (!asyncOperations.owns(operation)) {
        await discard(taskId);
        throw new SupersededPreparation();
      }

      const snapshot = mapProtocolTaskSnapshot(task).snapshot;
      const lease = replacementTaskId
        ? newTaskController.retainReplacement({
            attachmentResources,
            preparationKey,
            snapshot,
            staleTaskId: replacementTaskId,
          })
        : newTaskController.retain({
            attachmentResources,
            preparationKey,
            snapshot,
          });
      if (!lease) throw new SupersededPreparation();
      dispatch(replacementTaskId
        ? { type: "newTask:replaced", staleTaskId: replacementTaskId, taskId }
        : { type: "newTask:prepared", taskId });
      completedPreparationKey.current = preparationKey;
      failedPreparationKey.current = undefined;
      return { taskId, task };
    });
    const pending = { key: preparationKey, promise, settled: false };
    pendingPreparation.current = pending;
    // The options catalog belongs to the Prepared Task response, but the user
    // already waits for it once acquisition starts. Represent that interval
    // immediately instead of rendering a false-empty options slot.
    dispatch({ type: "newTask:configOptions:start" });

    void promise.catch((error) => {
      if (error instanceof SupersededPreparation) return;
      if (!asyncOperations.owns(operation)) return;
      failedPreparationKey.current = preparationKey;
      dispatch({
        type: "submit:error",
        message: error instanceof Error ? error.message : "Unable to prepare Task.",
      });
    }).then(() => undefined, () => undefined).finally(() => {
      pending.settled = true;
      // Successful preparations stay available so immediate submit/upload can reuse
      // the exact Task even before React publishes its mapped snapshot.
      if (newTaskController.ownsPreparation(preparationKey)) return;
      if (pendingPreparation.current?.promise === promise) {
        pendingPreparation.current = undefined;
      }
    });
  }, [
    backendConnection,
    backendReady,
    attachmentResources,
    asyncOperations,
    bootstrap.surface,
    bootstrap.taskId,
    clientInstanceId,
    dispatch,
    pendingPreparation,
    preparationKey,
    newTaskController,
    preparedTaskMatches,
    replicaEpoch,
    replacementTaskId,
    retainedSnapshot,
    state,
    startAttempt,
  ]);

}

class SupersededPreparation extends Error {}
