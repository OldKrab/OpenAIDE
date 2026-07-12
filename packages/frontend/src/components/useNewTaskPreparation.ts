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
import type { NewTaskController } from "./newTaskController";

export type PendingNewTaskPreparation = {
  key: string;
  promise: Promise<PendingNewTaskPreparationResult>;
};

type NewTaskPreparationOptions = {
  backendConnection?: AppControllerBackendConnection;
  backendReady: boolean;
  bootstrap: WebviewBootstrap;
  attachmentResources?: ComposerAttachmentResourceOwner;
  dispatch: Dispatch<AppAction>;
  latestOptionsRequestKey: MutableRefObject<string | undefined>;
  pendingPreparation: MutableRefObject<PendingNewTaskPreparation | undefined>;
  newTaskController: NewTaskController;
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
  dispatch,
  latestOptionsRequestKey,
  pendingPreparation,
  newTaskController,
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
  const retainedSnapshot = newTaskController.getSnapshot();
  const preparedTaskMatches = Boolean(
    retainedSnapshot
      && retainedSnapshot.lifecycle === "new"
      && retainedSnapshot.task.project_id === state.newTask.selection.projectId
      && retainedSnapshot.task.agent_id === state.newTask.selection.agentId,
  );
  if (isNewTaskRoute && preparedTaskMatches && preparationKey) {
    completedPreparationKey.current = preparationKey;
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
    const requestReplicaEpoch = replicaEpoch;
    const previousPreparation = pendingPreparation.current?.promise;
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
      if (currentPreparationKey.current !== preparationKey) {
        await discard(taskId);
        throw new SupersededPreparation();
      }

      const snapshot = mapProtocolTaskSnapshot(task).snapshot;
      const lease = newTaskController.retain({
        attachmentResources,
        preparationKey,
        snapshot,
      });
      if (!lease) throw new SupersededPreparation();
      dispatch({ type: "newTask:prepared", taskId });
      completedPreparationKey.current = preparationKey;
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
      if (newTaskController.ownsPreparation(preparationKey)) return;
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
    dispatch,
    latestOptionsRequestKey,
    pendingPreparation,
    preparationKey,
    newTaskController,
    preparedTaskMatches,
    replicaEpoch,
    retainedSnapshot,
    state,
    startAttempt,
  ]);

}

class SupersededPreparation extends Error {}
