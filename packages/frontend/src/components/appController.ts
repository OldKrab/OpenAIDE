import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import type { Dispatch } from "react";
import type { AppPreferencesRecord, TaskSummary } from "@openaide/app-shell-contracts";
import { defaultAgent } from "@openaide/app-shell-contracts";
import {
  getBackendConnection,
  getBootstrap,
  postHostMessage,
} from "../services/hostBridge";
import { clientInstanceIdForBootstrap } from "../services/backendInitialization";
import { readPendingTaskSendRecovery } from "../services/pendingTaskSendRecovery";
import {
  executeTaskSendAttempt,
  isTaskSendOutcomeUnknown,
  TASK_SEND_OUTCOME_UNKNOWN_MESSAGE,
} from "../services/taskSendAttempt";
import {
  appReducer,
  bindAppServerReplicaEpoch,
  type AppAction,
  type SnapshotIntent,
} from "../state/appReducer";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import type { AgentOption } from "../state/composerOptions";
import { sendWebviewTelemetry } from "../state/hostMessageRouter";
import {
  agentProjectRequestKey,
  shouldLoadNativeSessions,
} from "../state/surfaceRouting";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import { createInitialState, type AppState } from "../state/store";
import { createAppCallbacks, type AppControllerCallbacks } from "./appControllerCallbacks";
import type { NewTaskStartAttempt } from "./appControllerCallbackTypes";
import {
  useAppControllerBackendLifecycle,
  type AppControllerBackendConnection,
  type AppServerReplicaTransition,
  type BackendConnectionState,
} from "./appControllerBackendLifecycle";
import { appControllerDerivedStateDeps, deriveAppControllerState } from "./appControllerDerivedState";
import { createRequestControllerNativeSessions } from "./appControllerNativeSessions";
import {
  invalidateAppControllerReplicaRequests,
  useAppControllerRefs,
} from "./appControllerRefs";
import { useSettingsRouteRefresh } from "./appControllerRouting";
import { useNewTaskPreparation, type PendingNewTaskPreparation } from "./useNewTaskPreparation";
import { useTaskAttentionReadReceipt } from "./useTaskAttentionReadReceipt";
import { newTaskProjectIdForRequests } from "./newTaskRequestContext";
import { useComposerAttachmentResources } from "./useComposerAttachmentResources";
import { newTaskPreparationKey } from "../state/newTaskPreparationContext";
import { PreparedTaskOwnership } from "./preparedTaskOwnership";
export type AppController = {
  activeTask?: TaskSummary;
  activeNavigationTaskId?: string;
  agents?: AgentOption[];
  backendReady: boolean;
  backendConnectionState: BackendConnectionState;
  bootstrap: WebviewBootstrap;
  callbacks: AppControllerCallbacks;
  createSnapshotRequestId: (taskId?: string, intent?: SnapshotIntent) => number;
  dispatch: Dispatch<AppAction>;
  preferences: AppPreferencesRecord;
  retryTaskOpen: () => void;
  state: AppState;
  visibleTasks: AppState["tasks"];
};

export type AppControllerOptions = {
  backendConnection?: AppControllerBackendConnection;
};
export function useAppController({ backendConnection }: AppControllerOptions = {}) {
  const backendConnectionRef = useMemo(() => backendConnection ?? getBackendConnection(), [backendConnection]);
  const initialBootstrap = useMemo(() => getBootstrap(), []);
  const clientInstanceId = useMemo(() => clientInstanceIdForBootstrap(initialBootstrap), [initialBootstrap]);
  const [state, dispatch] = useReducer(appReducer, undefined, createInitialState);
  const [preferences, setPreferences] = useState<AppPreferencesRecord>(initialBootstrap.preferences ?? { composer_submit_shortcut: "enter" });
  const [agents, setAgents] = useState<AgentOption[] | undefined>(undefined);
  const currentAgentId = useRef(state.newTask.selection.agentId);
  currentAgentId.current = state.newTask.selection.agentId;
  const pendingPreparedNewTask = useRef<PendingNewTaskPreparation | undefined>(undefined);
  const preparedTaskOwnership = useMemo(() => new PreparedTaskOwnership(), []);
  const newTaskStartAttempt = useRef<NewTaskStartAttempt | undefined>(undefined);
  const currentNewTaskPreparationKey = useRef<string | undefined>(undefined);
  currentNewTaskPreparationKey.current = newTaskPreparationKey(state);
  const recoveredTaskSendAttempts = useRef(new Set<string>());
  const attachmentResourcesRef = useRef<ReturnType<typeof useComposerAttachmentResources> | undefined>(undefined);
  const controllerRefs = useAppControllerRefs();
  const {
    latestNativeSessionSelection,
    latestNavigationSessionKey,
    latestOptionsRequestKey,
    latestSessionListRequestId,
    nextChatPageRequestGeneration,
    nextSessionListRequestId,
  } = controllerRefs;
  const handleReplicaChanged = useCallback((transition: AppServerReplicaTransition) => {
    if (!transition.previous) return;
    invalidateAppControllerReplicaRequests(controllerRefs);
    pendingPreparedNewTask.current = undefined;
    recoveredTaskSendAttempts.current.clear();
    attachmentResourcesRef.current?.replaceReplica();
    if (!transition.rootChanged) return;
    // Task ids and cleanup tombstones can collide across roots. Forget them
    // locally without issuing cleanup requests into the replacement root.
    preparedTaskOwnership.replaceStateRoot();
    newTaskStartAttempt.current = undefined;
  }, [controllerRefs, preparedTaskOwnership]);
  const {
    acceptSnapshotRequest,
    backendInitialized,
    backendConnectionState,
    backendReady,
    beginNavigationChange,
    bootstrap,
    createSnapshotRequestId,
    currentNavigationGeneration,
    replicaEpoch,
    retryTaskOpen,
  } = useAppControllerBackendLifecycle({
    backendConnection: backendConnectionRef,
    currentAgentId,
    dispatch,
    initialBootstrap,
    onReplicaChanged: handleReplicaChanged,
    refs: controllerRefs,
    setAgents,
    setPreferences,
    state,
  });
  const replicaDispatch = useMemo(
    () => bindAppServerReplicaEpoch(dispatch, replicaEpoch),
    [dispatch, replicaEpoch],
  );
  const attachmentResources = useComposerAttachmentResources({
    backendConnection: backendConnectionRef,
    clientInstanceId,
    dispatch: replicaDispatch,
    state,
    taskSurfaceMounted: bootstrap.surface === "task",
  });
  attachmentResourcesRef.current = attachmentResources;
  const newTaskBootstrapProjectId = bootstrap.surface === "task" && !bootstrap.taskId
    ? bootstrap.projectId
    : undefined;
  useNewTaskPreparation({
    attachmentResources,
    backendConnection: backendConnectionRef,
    backendReady,
    bootstrap,
    currentNavigationGeneration,
    dispatch: replicaDispatch,
    latestOptionsRequestKey,
    pendingPreparation: pendingPreparedNewTask,
    preparedTaskOwnership,
    replicaEpoch,
    startAttempt: newTaskStartAttempt,
    state,
  });
  const pendingPreparedNewTaskForKey = (key: string) =>
    pendingPreparedNewTask.current?.key === key ? pendingPreparedNewTask.current.promise : undefined;
  const requestNativeSessions = createRequestControllerNativeSessions({
    backendConnection: backendConnectionRef,
    dispatch: replicaDispatch,
    getAgentId: () => state.newTask.selection.agentId,
    getExistingSessionIds: () => state.newTask.nativeSessions.items.map((session) => session.session_id),
    getProjectId: () => state.newTask.selection.projectId,
    latestSessionListRequestId,
    nextSessionListRequestId,
    onFailure: (failure) => sendWebviewTelemetry(postHostMessage, "native_sessions_load_failed", {
      surface: bootstrap.surface,
      request: failure.request,
      session_list_request_id: failure.requestId,
      agent_id: failure.agentId,
      project_id: failure.projectId,
      error_name: failure.errorName,
      error_code: failure.errorCode,
      error_message: failure.errorMessage,
    }),
  });
  useEffect(() => {
    if (
      bootstrap.surface === "task" &&
      !bootstrap.taskId &&
      newTaskBootstrapProjectId &&
      state.newTask.selection.projectId !== newTaskBootstrapProjectId
    ) {
      replicaDispatch({ type: "newTask:projectId", projectId: newTaskBootstrapProjectId });
    }
  }, [bootstrap.surface, bootstrap.taskId, newTaskBootstrapProjectId, state.newTask.selection.projectId]);

  useEffect(() => {
    if (bootstrap.surface !== "task" || bootstrap.taskId || !agents?.length) return;
    const selected = agents.find((agent) => agent.id === state.newTask.selection.agentId);
    if (selected && selected.enabled !== false) return;
    const fallback = agents.find((agent) => agent.enabled !== false) ?? defaultAgent;
    latestOptionsRequestKey.current = undefined;
    replicaDispatch({ type: "newTask:agent", agentId: fallback.id, agentLabel: fallback.label });
  }, [agents, bootstrap.surface, bootstrap.taskId, state.newTask.selection.agentId]);

  useEffect(() => {
    if (
      bootstrap.surface !== "task" ||
      !bootstrap.taskId ||
      !backendReady ||
      !backendConnectionRef?.request ||
      !state.appServerStateRootId ||
      !state.snapshot ||
      state.snapshot.task.task_id !== bootstrap.taskId
    ) {
      return;
    }
    const recovery = readPendingTaskSendRecovery(
      state.appServerStateRootId,
      clientInstanceId,
      bootstrap.taskId,
    );
    if (!recovery) return;
    const recoveryKey = `${recovery.stateRootId}:${recovery.taskId}:${recovery.idempotencyKey}`;
    if (recoveredTaskSendAttempts.current.has(recoveryKey)) return;
    recoveredTaskSendAttempts.current.add(recoveryKey);
    replicaDispatch({
      type: "taskInput:restoreSend",
      taskId: recovery.taskId,
      input: recovery.renderState,
      idempotencyKey: recovery.idempotencyKey,
    });
    void executeTaskSendAttempt({
      attempt: recovery,
      backendConnection: backendConnectionRef,
      refreshRevisionOnConflict: true,
    }).then(({ attempt, result }) => {
      replicaDispatch({
        type: "snapshot",
        snapshot: mapProtocolTaskSnapshot(result.task).snapshot,
        intent: "refresh",
      });
      replicaDispatch({
        type: "taskSend:accepted",
        taskId: recovery.taskId,
        idempotencyKey: attempt.idempotencyKey,
        userMessageId: result.userMessageId,
      });
    }).catch((error) => {
      if (isTaskSendOutcomeUnknown(error)) {
        replicaDispatch({
          type: "taskInput:sendUncertain",
          taskId: recovery.taskId,
          idempotencyKey: recovery.idempotencyKey,
          message: TASK_SEND_OUTCOME_UNKNOWN_MESSAGE,
        });
        return;
      }
      replicaDispatch({
        type: "taskInput:sendError",
        taskId: recovery.taskId,
        idempotencyKey: recovery.idempotencyKey,
        message: error instanceof Error ? error.message : "Unable to recover submitted message.",
      });
    });
  }, [
    backendConnectionRef,
    backendReady,
    bootstrap.surface,
    bootstrap.taskId,
    clientInstanceId,
    state.appServerStateRootId,
    state.snapshot?.task.task_id,
  ]);

  useEffect(() => {
    const snapshotTaskId = state.snapshot?.task.task_id;
    const snapshotHasPendingInput = snapshotTaskId
      ? state.taskInputs[snapshotTaskId]?.pending !== undefined
      : false;
    const snapshotHasPendingRecovery = state.appServerStateRootId && snapshotTaskId
      ? readPendingTaskSendRecovery(state.appServerStateRootId, clientInstanceId, snapshotTaskId) !== undefined
      : false;
    if (
      bootstrap.surface !== "task" ||
      !bootstrap.taskId ||
      !state.snapshot ||
      state.snapshot.task.has_messages ||
      state.snapshot.task.status !== "inactive" ||
      snapshotHasPendingInput ||
      snapshotHasPendingRecovery
    ) {
      return;
    }
    postHostMessage({
      type: "surface.openNewTask",
      payload: state.snapshot.task.project_id
        ? { project_id: state.snapshot.task.project_id }
        : undefined,
    });
  }, [
    bootstrap.surface,
    bootstrap.taskId,
    state.snapshot?.task.task_id,
    state.snapshot?.task.has_messages,
    state.snapshot?.task.status,
    state.snapshot?.task.project_id,
    state.snapshot ? state.taskInputs[state.snapshot.task.task_id]?.pending : undefined,
    clientInstanceId,
    state.appServerStateRootId,
  ]);
  useSettingsRouteRefresh({
    backendConnectionRef,
    backendInitialized,
    bootstrap,
    currentAgentId,
    dispatch: replicaDispatch,
    setAgents,
    state,
  });
  useTaskAttentionReadReceipt({
    backendConnection: backendConnectionRef,
    dispatch: replicaDispatch,
    revision: state.snapshot?.revision,
    taskId: bootstrap.surface === "task" ? state.snapshot?.task.task_id : undefined,
    unread: state.snapshot?.task.unread === true,
  });
  useEffect(() => {
    const projectId = newTaskProjectIdForRequests(state, newTaskBootstrapProjectId);
    if (
      !backendReady ||
      !shouldLoadNativeSessions(
        bootstrap,
        projectId,
      )
    ) {
      return;
    }
    if (!projectId) return;
    const key = agentProjectRequestKey(state.newTask.selection.agentId, projectId);
    if (latestNavigationSessionKey.current === key) return;
    latestNavigationSessionKey.current = key;
    latestNativeSessionSelection.current = {
      agentId: state.newTask.selection.agentId,
      workspaceRoot: state.newTask.selection.workspaceRoot,
    };
    requestNativeSessions();
  }, [
    backendReady,
    bootstrap.surface,
    replicaEpoch,
    state.newTask.selection.agentId,
    state.newTask.selection.projectId,
    state.newTask.selection.workspaceRoot,
    state.projects,
    state.tasks,
    newTaskBootstrapProjectId,
  ]);

  const derivedStateDeps = appControllerDerivedStateDeps(state);
  const { activeNavigationTaskId, activeTask, hasActiveTask, visibleTasks } = useMemo(
    () => deriveAppControllerState(state),
    derivedStateDeps,
  );

  useEffect(() => {
    if (!state.snapshot) return;
    sendWebviewTelemetry(postHostMessage, "task_rendered", {
      surface: bootstrap.surface,
      task_id: state.snapshot.task.task_id,
      task_status: state.snapshot.task.status,
      chat_items: state.snapshot.chat.items.length,
      has_active_task: hasActiveTask,
    });
  }, [hasActiveTask, bootstrap.surface, state.snapshot?.task.task_id, state.snapshot?.task.status, state.snapshot?.chat.items.length]);

  const callbacks = createAppCallbacks({
    acceptTaskOpen: acceptSnapshotRequest,
    attachmentResources,
    backendConnection: backendConnectionRef,
    beginNavigationChange,
    clientInstanceId,
    createChatPageRequestGeneration: () => {
      nextChatPageRequestGeneration.current += 1;
      return nextChatPageRequestGeneration.current;
    },
    createSnapshotRequestId,
    currentNavigationGeneration,
    currentNewTaskPreparationKey: () => currentNewTaskPreparationKey.current,
    dispatch: replicaDispatch,
    latestOptionsRequestKey,
    newTaskStartAttempt,
    pendingPreparedNewTask: pendingPreparedNewTaskForKey,
    preparedTaskOwnership,
    requestNativeSessions,
    setAgents,
    setPreferences,
    state,
  });

  return {
    activeNavigationTaskId,
    activeTask,
    agents,
    backendConnectionState,
    backendReady,
    bootstrap,
    callbacks,
    createSnapshotRequestId,
    dispatch: replicaDispatch,
    preferences,
    retryTaskOpen,
    state,
    visibleTasks,
  };
}
