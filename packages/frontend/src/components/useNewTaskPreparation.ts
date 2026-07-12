import { useEffect, useRef, type Dispatch, type MutableRefObject } from "react";
import {
  TASK_CREATE,
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
import { newTaskPreparationKey, taskCreateParams } from "../state/newTaskPreparationContext";
import type { PreparedTaskOwnership } from "./preparedTaskOwnership";

export type PendingNewTaskPreparation = {
  key: string;
  promise: Promise<PendingNewTaskPreparationResult>;
};

type NewTaskPreparationOptions = {
  backendConnection?: AppControllerBackendConnection;
  backendReady: boolean;
  bootstrap: WebviewBootstrap;
  attachmentResources?: ComposerAttachmentResourceOwner;
  currentNavigationGeneration: () => number;
  dispatch: Dispatch<AppAction>;
  latestOptionsRequestKey: MutableRefObject<string | undefined>;
  pendingPreparation: MutableRefObject<PendingNewTaskPreparation | undefined>;
  preparedTaskOwnership: PreparedTaskOwnership;
  replicaEpoch: number;
  startAttempt: MutableRefObject<NewTaskStartAttempt | undefined>;
  state: AppState;
};

/** Starts the Task/session boundary once the required new-task context exists. */
export function useNewTaskPreparation({
  attachmentResources,
  backendConnection,
  backendReady,
  bootstrap,
  currentNavigationGeneration,
  dispatch,
  latestOptionsRequestKey,
  pendingPreparation,
  preparedTaskOwnership,
  replicaEpoch,
  startAttempt,
  state,
}: NewTaskPreparationOptions) {
  const preparationKey = newTaskPreparationKey(state);
  const completedPreparationKey = useRef<string | undefined>(undefined);
  const currentPreparationKey = useRef(preparationKey);
  const failedPreparationKey = useRef<string | undefined>(undefined);
  const currentReplicaEpoch = useRef(replicaEpoch);
  if (currentReplicaEpoch.current !== replicaEpoch) {
    currentReplicaEpoch.current = replicaEpoch;
    pendingPreparation.current = undefined;
    completedPreparationKey.current = undefined;
    failedPreparationKey.current = undefined;
  }
  currentPreparationKey.current = preparationKey;
  const isNewTaskRoute = bootstrap.surface === "task" && !bootstrap.taskId;
  const previousBootstrap = useRef(bootstrap);
  const enteredNewTaskRoute = isNewTaskRoute && previousBootstrap.current !== bootstrap;
  previousBootstrap.current = bootstrap;
  if (!isNewTaskRoute) completedPreparationKey.current = undefined;
  if (enteredNewTaskRoute) {
    pendingPreparation.current = undefined;
    completedPreparationKey.current = undefined;
  }
  if (
    state.snapshot?.task.has_messages
  ) {
    preparedTaskOwnership.confirmSentTask(state.snapshot.task.task_id);
    pendingPreparation.current = undefined;
    completedPreparationKey.current = preparationKey;
  }
  const preparedTaskMatches = Boolean(
    state.snapshot
      && !state.snapshot.task.has_messages
      && state.snapshot.task.project_id === state.newTask.selection.projectId
      && state.snapshot.task.agent_id === state.newTask.selection.agentId,
  );

  useEffect(() => {
    if (
      bootstrap.surface !== "task"
      || bootstrap.taskId
      || !backendReady
      || !backendConnection?.request
      || !preparationKey
      || state.snapshot?.task.has_messages
      || preparedTaskMatches
      || completedPreparationKey.current === preparationKey
      || failedPreparationKey.current === preparationKey
      || pendingPreparation.current?.key === preparationKey
    ) {
      return;
    }

    const request = backendConnection.request;
    const requestReplicaEpoch = replicaEpoch;
    const navigationGeneration = currentNavigationGeneration();
    const previousPreparation = pendingPreparation.current?.promise;
    const staleTaskId = state.snapshot && !state.snapshot.task.has_messages
      ? state.snapshot.task.task_id as TaskId
      : undefined;
    const discard = (taskId: TaskId) => preparedTaskOwnership.discard({
      attachmentResources,
      dispatch,
      lease: preparedTaskOwnership.currentLease(taskId),
      request,
      taskId,
    });
    const promise = (previousPreparation
      ? previousPreparation.catch(() => undefined)
      : Promise.resolve()
    ).then(async () => {
      if (currentReplicaEpoch.current !== requestReplicaEpoch) {
        throw new SupersededPreparation();
      }
      if (staleTaskId) await discard(staleTaskId);
      if (currentPreparationKey.current !== preparationKey) {
        throw new SupersededPreparation();
      }

      const projectId = state.newTask.selection.projectId;
      if (!projectId) throw new SupersededPreparation();
      const task = (await request(TASK_CREATE, taskCreateParams(state, projectId))).task;
      if (currentReplicaEpoch.current !== requestReplicaEpoch) {
        throw new SupersededPreparation();
      }
      const taskId = task.task.taskId as TaskId;
      const cancelledAttempt = startAttempt.current?.cancelled ? startAttempt.current : undefined;
      if (cancelledAttempt) {
        cancelledAttempt.taskId = taskId;
        await discard(taskId);
        if (startAttempt.current === cancelledAttempt) startAttempt.current = undefined;
        return { taskId, task };
      }
      if (
        currentPreparationKey.current !== preparationKey
        || currentNavigationGeneration() !== navigationGeneration
      ) {
        await discard(taskId);
        throw new SupersededPreparation();
      }

      preparedTaskOwnership.claim({
        attachmentResources,
        preparationKey,
        taskId,
      });
      const intent = currentNavigationGeneration() === navigationGeneration ? "open" : "refresh";
      dispatch({ type: "snapshot", snapshot: mapProtocolTaskSnapshot(task).snapshot, intent });
      dispatch({ type: "newTask:prepared", taskId });
      latestOptionsRequestKey.current = undefined;
      failedPreparationKey.current = undefined;
      return { taskId, task };
    });
    pendingPreparation.current = { key: preparationKey, promise };

    void promise.catch((error) => {
      if (error instanceof SupersededPreparation) return;
      if (currentReplicaEpoch.current !== requestReplicaEpoch) return;
      failedPreparationKey.current = preparationKey;
      dispatch({
        type: "submit:error",
        message: error instanceof Error ? error.message : "Unable to prepare Task.",
      });
    }).then(() => undefined, () => undefined).finally(() => {
      // Successful preparations stay available so immediate submit/upload can reuse
      // the exact Task even before React publishes its mapped snapshot.
      if (preparedTaskOwnership.ownsPreparation(preparationKey)) return;
      if (pendingPreparation.current?.promise === promise) {
        pendingPreparation.current = undefined;
      }
    });
  }, [
    backendConnection,
    backendReady,
    attachmentResources,
    bootstrap.surface,
    bootstrap.taskId,
    currentNavigationGeneration,
    dispatch,
    latestOptionsRequestKey,
    pendingPreparation,
    preparationKey,
    preparedTaskOwnership,
    preparedTaskMatches,
    replicaEpoch,
    state,
    startAttempt,
  ]);

  useEffect(() => {
    if (
      !isNewTaskRoute
      || state.newTask.submitting
      || !preparedTaskMatches
      || !preparationKey
      || !state.snapshot
      || !preparedTaskOwnership.isDisposable(state.snapshot.task.task_id)
    ) return;
    preparedTaskOwnership.claim({
      attachmentResources,
      preparationKey,
      taskId: state.snapshot.task.task_id as TaskId,
    });
  }, [
    attachmentResources,
    isNewTaskRoute,
    preparationKey,
    preparedTaskMatches,
    preparedTaskOwnership,
    state.newTask.submitting,
    state.snapshot?.task.task_id,
  ]);

  useEffect(() => {
    if (isNewTaskRoute || state.newTask.submitting || !backendConnection?.request) return;
    const taskId = preparedTaskOwnership.currentTaskId();
    if (!taskId) return;
    void preparedTaskOwnership.discard({
      attachmentResources,
      dispatch,
      lease: preparedTaskOwnership.currentLease(taskId),
      request: backendConnection.request,
      taskId,
    });
  }, [
    attachmentResources,
    backendConnection,
    dispatch,
    isNewTaskRoute,
    preparedTaskOwnership,
    state.newTask.submitting,
  ]);

}

class SupersededPreparation extends Error {}
