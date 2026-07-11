import { useEffect, useRef, type Dispatch, type MutableRefObject } from "react";
import {
  TASK_CREATE,
  TASK_DISCARD,
  type TaskId,
} from "@openaide/app-server-client";
import type { AppAction } from "../state/appReducer";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import type { AppState } from "../state/store";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import type { AppControllerBackendConnection } from "./appControllerBackendLifecycle";
import type { PendingNewTaskPreparationResult } from "./appControllerCallbackTypes";
import type { NewTaskStartAttempt } from "./appControllerCallbackTypes";
import { newTaskPreparationKey, taskCreateParams } from "./newTaskPreparationContext";

export type PendingNewTaskPreparation = {
  key: string;
  promise: Promise<PendingNewTaskPreparationResult>;
};

type NewTaskPreparationOptions = {
  backendConnection?: AppControllerBackendConnection;
  backendReady: boolean;
  bootstrap: WebviewBootstrap;
  currentNavigationGeneration: () => number;
  dispatch: Dispatch<AppAction>;
  latestOptionsRequestKey: MutableRefObject<string | undefined>;
  pendingPreparation: MutableRefObject<PendingNewTaskPreparation | undefined>;
  startAttempt: MutableRefObject<NewTaskStartAttempt | undefined>;
  state: AppState;
};

/** Starts the Task/session boundary once the required new-task context exists. */
export function useNewTaskPreparation({
  backendConnection,
  backendReady,
  bootstrap,
  currentNavigationGeneration,
  dispatch,
  latestOptionsRequestKey,
  pendingPreparation,
  startAttempt,
  state,
}: NewTaskPreparationOptions) {
  const preparationKey = newTaskPreparationKey(state);
  const completedPreparationKey = useRef<string | undefined>(undefined);
  const currentPreparationKey = useRef(preparationKey);
  const discardedTaskIds = useRef(new Set<string>());
  const failedPreparationKey = useRef<string | undefined>(undefined);
  const latestRequest = useRef(backendConnection?.request);
  const latestSubmitting = useRef(state.newTask.submitting);
  const ownedPreparedTaskId = useRef<TaskId | undefined>(undefined);
  currentPreparationKey.current = preparationKey;
  latestRequest.current = backendConnection?.request;
  latestSubmitting.current = state.newTask.submitting;
  const isNewTaskRoute = bootstrap.surface === "task" && !bootstrap.taskId;
  if (!isNewTaskRoute) completedPreparationKey.current = undefined;
  if (
    isNewTaskRoute
    && ownedPreparedTaskId.current
    && !state.snapshot
    && !state.newTask.submitting
  ) {
    ownedPreparedTaskId.current = undefined;
    pendingPreparation.current = undefined;
    completedPreparationKey.current = undefined;
  }
  if (
    ownedPreparedTaskId.current
    && state.snapshot?.task.task_id === ownedPreparedTaskId.current
    && state.snapshot.task.has_messages
  ) {
    ownedPreparedTaskId.current = undefined;
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
      || preparedTaskMatches
      || completedPreparationKey.current === preparationKey
      || failedPreparationKey.current === preparationKey
      || pendingPreparation.current?.key === preparationKey
    ) {
      return;
    }

    const request = backendConnection.request;
    const navigationGeneration = currentNavigationGeneration();
    const previousPreparation = pendingPreparation.current?.promise;
    const staleTaskId = state.snapshot && !state.snapshot.task.has_messages
      ? state.snapshot.task.task_id as TaskId
      : undefined;
    const discard = async (taskId: TaskId) => {
      if (discardedTaskIds.current.has(taskId)) return;
      discardedTaskIds.current.add(taskId);
      if (ownedPreparedTaskId.current === taskId) ownedPreparedTaskId.current = undefined;
      await request(TASK_DISCARD, { taskId });
      dispatch({ type: "task:list:remove", taskId });
    };
    const promise = (previousPreparation
      ? previousPreparation.catch(() => undefined)
      : Promise.resolve()
    ).then(async () => {
      if (staleTaskId) await discard(staleTaskId);
      if (currentPreparationKey.current !== preparationKey) {
        throw new SupersededPreparation();
      }

      const projectId = state.newTask.selection.projectId;
      if (!projectId) throw new SupersededPreparation();
      const task = (await request(TASK_CREATE, taskCreateParams(state, projectId))).task;
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

      const intent = currentNavigationGeneration() === navigationGeneration ? "open" : "refresh";
      dispatch({ type: "snapshot", snapshot: mapProtocolTaskSnapshot(task).snapshot, intent });
      dispatch({ type: "newTask:prepared", taskId });
      latestOptionsRequestKey.current = undefined;
      failedPreparationKey.current = undefined;
      ownedPreparedTaskId.current = taskId;
      return { taskId, task };
    });
    pendingPreparation.current = { key: preparationKey, promise };

    void promise.catch((error) => {
      if (error instanceof SupersededPreparation) return;
      failedPreparationKey.current = preparationKey;
      dispatch({
        type: "submit:error",
        message: error instanceof Error ? error.message : "Unable to prepare Task.",
      });
    }).then(() => undefined, () => undefined).finally(() => {
      // Successful preparations stay available so immediate submit/upload can reuse
      // the exact Task even before React publishes its mapped snapshot.
      if (ownedPreparedTaskId.current) return;
      if (pendingPreparation.current?.promise === promise) {
        pendingPreparation.current = undefined;
      }
    });
  }, [
    backendConnection,
    backendReady,
    bootstrap.surface,
    bootstrap.taskId,
    currentNavigationGeneration,
    dispatch,
    latestOptionsRequestKey,
    pendingPreparation,
    preparationKey,
    preparedTaskMatches,
    state,
    startAttempt,
  ]);

  useEffect(() => {
    if (bootstrap.surface === "task" && !bootstrap.taskId) return;
    const taskId = ownedPreparedTaskId.current;
    if (!taskId) return;
    ownedPreparedTaskId.current = undefined;
    pendingPreparation.current = undefined;
    if (state.newTask.submitting || !backendConnection?.request) return;

    void backendConnection.request(TASK_DISCARD, { taskId }).then(() => {
      discardedTaskIds.current.add(taskId);
      dispatch({ type: "task:list:remove", taskId });
    }).catch(() => {
      // Navigation remains immediate; the App Server owns eventual empty-task cleanup.
    });
  }, [
    backendConnection,
    bootstrap.surface,
    bootstrap.taskId,
    dispatch,
    state.newTask.submitting,
  ]);

  useEffect(() => () => {
    const taskId = ownedPreparedTaskId.current;
    const request = latestRequest.current;
    if (!taskId || latestSubmitting.current || !request) return;
    ownedPreparedTaskId.current = undefined;
    pendingPreparation.current = undefined;
    discardedTaskIds.current.add(taskId);
    void request(TASK_DISCARD, { taskId }).catch(() => {
      // Disconnect and abandoned-task cleanup remain authoritative if unload wins.
    });
  }, [pendingPreparation]);
}

class SupersededPreparation extends Error {}
