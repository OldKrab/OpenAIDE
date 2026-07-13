import { useCallback, useEffect, useMemo, useReducer, useRef, useState, useSyncExternalStore } from "react";
import type { Dispatch } from "react";
import type { AppPreferencesRecord, TaskSummary } from "@openaide/app-shell-contracts";
import { defaultAgent } from "@openaide/app-shell-contracts";
import {
  getBackendConnection,
  getBootstrap,
  openNewTaskSurface,
  postHostMessage,
} from "../services/hostBridge";
import { clientInstanceIdForBootstrap } from "../services/backendInitialization";
import { retainNewTaskContext } from "../state/newTaskSelectionDefaults";
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
import { NewTaskController } from "./newTaskController";
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
  newTaskSnapshot?: import("@openaide/app-shell-contracts").TaskSnapshot;
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
  const currentNewTaskContext = useRef({
    projectId: state.newTask.selection.projectId,
    agentId: state.newTask.selection.agentId || undefined,
  });
  currentNewTaskContext.current = {
    projectId: state.newTask.selection.projectId,
    agentId: state.newTask.selection.agentId || undefined,
  };
  const pendingPreparedNewTask = useRef<PendingNewTaskPreparation | undefined>(undefined);
  const newTaskController = useMemo(() => new NewTaskController(), []);
  const newTaskSnapshot = useSyncExternalStore(newTaskController.subscribe, newTaskController.getSnapshot);
  const newTaskStartAttempt = useRef<NewTaskStartAttempt | undefined>(undefined);
  const attachmentResourcesRef = useRef<ReturnType<typeof useComposerAttachmentResources> | undefined>(undefined);
  const controllerRefs = useAppControllerRefs();
  const {
    asyncOperations,
    latestNavigationSessionKey,
  } = controllerRefs;
  const handleReplicaChanged = useCallback((transition: AppServerReplicaTransition) => {
    if (!transition.previous) return;
    invalidateAppControllerReplicaRequests(controllerRefs);
    pendingPreparedNewTask.current = undefined;
    attachmentResourcesRef.current?.replaceReplica();
    if (!transition.rootChanged) return;
    // Task ids and cleanup tombstones can collide across roots. Forget them
    // locally without issuing cleanup requests into the replacement root.
    newTaskController.replaceStateRoot();
    newTaskStartAttempt.current = undefined;
  }, [controllerRefs, newTaskController]);
  const {
    acceptSnapshotRequest,
    backendInitialized,
    backendConnectionState,
    backendReady,
    bootstrap,
    createSnapshotRequestId,
    operationOwner,
    replicaEpoch,
    retryTaskOpen,
  } = useAppControllerBackendLifecycle({
    backendConnection: backendConnectionRef,
    currentAgentId,
    currentNewTaskContext,
    dispatch,
    initialBootstrap,
    newTaskController,
    newTaskId: newTaskSnapshot?.task.task_id,
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
  const newTaskDispatch = useCallback((action: AppAction) => {
    switch (action.type) {
      case "newTask:agent":
      case "newTask:project":
      case "newTask:projectId":
      case "newTask:workspace":
        replicaDispatch({
          ...action,
          newTaskId: action.newTaskId ?? newTaskSnapshot?.task.task_id,
        });
        return;
      default:
        replicaDispatch(action);
    }
  }, [newTaskSnapshot?.task.task_id, replicaDispatch]);
  const attachmentResources = useComposerAttachmentResources({
    backendConnection: backendConnectionRef,
    clientInstanceId,
    dispatch: newTaskDispatch,
    newTaskId: newTaskSnapshot?.task.task_id,
    state,
    taskSurfaceMounted: bootstrap.surface === "task",
  });
  attachmentResourcesRef.current = attachmentResources;
  const newTaskBootstrapProjectId = bootstrap.surface === "task" && !bootstrap.taskId
    ? bootstrap.projectId
    : undefined;
  useEffect(() => {
    if (!state.appServerStateRootId) return;
    retainNewTaskContext(state.appServerStateRootId, clientInstanceId, {
      projectId: state.newTask.selection.projectId,
      agentId: state.newTask.selection.agentId || undefined,
    });
  }, [
    clientInstanceId,
    state.appServerStateRootId,
    state.newTask.selection.agentId,
    state.newTask.selection.projectId,
  ]);
  useNewTaskPreparation({
    attachmentResources,
    asyncOperations: asyncOperations.current,
    backendConnection: backendConnectionRef,
    backendReady,
    bootstrap,
    dispatch: newTaskDispatch,
    pendingPreparation: pendingPreparedNewTask,
    newTaskController: newTaskController,
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
    asyncOperations: asyncOperations.current,
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
      newTaskDispatch({ type: "newTask:projectId", projectId: newTaskBootstrapProjectId });
    }
  }, [bootstrap.surface, bootstrap.taskId, newTaskBootstrapProjectId, state.newTask.selection.projectId]);

  useEffect(() => {
    if (bootstrap.surface !== "task" || bootstrap.taskId || !agents?.length) return;
    const selected = agents.find((agent) => agent.id === state.newTask.selection.agentId);
    if (selected && selected.enabled !== false) return;
    const fallback = agents.find((agent) => agent.enabled !== false) ?? defaultAgent;
    newTaskDispatch({ type: "newTask:agent", agentId: fallback.id, agentLabel: fallback.label });
  }, [agents, bootstrap.surface, bootstrap.taskId, state.newTask.selection.agentId]);

  useEffect(() => {
    const snapshotTaskId = state.snapshot?.task.task_id;
    const snapshotHasPendingInput = snapshotTaskId
      ? state.taskInputs[snapshotTaskId]?.pending !== undefined
      : false;
    if (
      bootstrap.surface !== "task" ||
      !bootstrap.taskId ||
      !state.snapshot ||
      state.snapshot.task.has_messages ||
      state.snapshot.task.status !== "inactive" ||
      snapshotHasPendingInput
    ) {
      return;
    }
    openNewTaskSurface(state.snapshot.task.project_id);
  }, [
    bootstrap.surface,
    bootstrap.taskId,
    state.snapshot?.task.task_id,
    state.snapshot?.task.has_messages,
    state.snapshot?.task.status,
    state.snapshot?.task.project_id,
    state.snapshot ? state.taskInputs[state.snapshot.task.task_id]?.pending : undefined,
  ]);
  useSettingsRouteRefresh({
    backendConnectionRef,
    backendInitialized,
    bootstrap,
    currentAgentId,
    dispatch: newTaskDispatch,
    setAgents,
    state,
  });
  useTaskAttentionReadReceipt({
    backendConnection: backendConnectionRef,
    dispatch: newTaskDispatch,
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

  const callbackState = bootstrap.surface === "task" && !bootstrap.taskId && newTaskSnapshot
    ? { ...state, snapshot: newTaskSnapshot }
    : state;

  const callbacks = createAppCallbacks({
    acceptTaskOpen: acceptSnapshotRequest,
    attachmentResources,
    asyncOperations: operationOwner,
    backendConnection: backendConnectionRef,
    clientInstanceId,
    createSnapshotRequestId,
    dispatch: newTaskDispatch,
    newTaskStartAttempt,
    pendingPreparedNewTask: pendingPreparedNewTaskForKey,
    newTaskController: newTaskController,
    requestNativeSessions,
    setAgents,
    setPreferences,
    state: callbackState,
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
    dispatch: newTaskDispatch,
    newTaskSnapshot,
    preferences,
    retryTaskOpen,
    state,
    visibleTasks,
  };
}
