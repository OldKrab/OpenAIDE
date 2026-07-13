import { useCallback, useMemo, useReducer, useRef, useState, useSyncExternalStore } from "react";
import type { Dispatch } from "react";
import type { AppPreferencesRecord, TaskSummary } from "@openaide/app-shell-contracts";
import {
  getBackendConnection,
  getBootstrap,
} from "../services/hostBridge";
import { clientInstanceIdForBootstrap } from "../services/backendInitialization";
import type { ComposerAttachmentResourceOwner } from "../services/attachmentResources";
import {
  appReducer,
  bindAppServerReplicaEpoch,
  type AppAction,
  type SnapshotIntent,
} from "../state/appReducer";
import type { AgentOption } from "../state/composerOptions";
import { AsyncOperationOwner } from "../state/asyncOperationOwner";
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
import { useSettingsRouteRefresh } from "./appControllerRouting";
import type { PendingNewTaskPreparation } from "./useNewTaskPreparation";
import { NewTaskController } from "./newTaskController";
import { useNewTaskWorkspace } from "./useNewTaskWorkspace";
import { useTaskWorkspace } from "./useTaskWorkspace";
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
  const attachmentResourcesRef = useRef<ComposerAttachmentResourceOwner | undefined>(undefined);
  const asyncOperations = useMemo(() => new AsyncOperationOwner(), []);
  const handleReplicaChanged = useCallback((transition: AppServerReplicaTransition) => {
    if (!transition.previous) return;
    asyncOperations.replaceReplica();
    pendingPreparedNewTask.current = undefined;
    attachmentResourcesRef.current?.replaceReplica();
    if (!transition.rootChanged) return;
    // Task ids and cleanup tombstones can collide across roots. Forget them
    // locally without issuing cleanup requests into the replacement root.
    newTaskController.replaceStateRoot();
    newTaskStartAttempt.current = undefined;
  }, [asyncOperations, newTaskController]);
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
    asyncOperations,
    backendConnection: backendConnectionRef,
    currentAgentId,
    currentNewTaskContext,
    dispatch,
    initialBootstrap,
    newTaskController,
    newTaskId: newTaskSnapshot?.task.task_id,
    onReplicaChanged: handleReplicaChanged,
    setAgents,
    setPreferences,
    state,
  });
  const replicaDispatch = useMemo(
    () => bindAppServerReplicaEpoch(dispatch, replicaEpoch),
    [dispatch, replicaEpoch],
  );
  const newTaskWorkspace = useNewTaskWorkspace({
    agents,
    asyncOperations,
    backendConnection: backendConnectionRef,
    backendReady,
    bootstrap,
    clientInstanceId,
    dispatch: replicaDispatch,
    newTaskController,
    newTaskSnapshot,
    pendingPreparation: pendingPreparedNewTask,
    replicaEpoch,
    startAttempt: newTaskStartAttempt,
    state,
  });
  const newTaskDispatch = newTaskWorkspace.dispatch;
  const attachmentResources = newTaskWorkspace.attachmentResources;
  attachmentResourcesRef.current = attachmentResources;
  useSettingsRouteRefresh({
    backendConnectionRef,
    backendInitialized,
    bootstrap,
    currentAgentId,
    dispatch: newTaskDispatch,
    setAgents,
    state,
  });
  const { activeNavigationTaskId, activeTask, visibleTasks } = useTaskWorkspace({
    backendConnection: backendConnectionRef,
    bootstrap,
    dispatch: newTaskDispatch,
    state,
  });

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
    pendingPreparedNewTask: newTaskWorkspace.pendingPreparationForKey,
    newTaskController: newTaskController,
    requestNativeSessions: newTaskWorkspace.requestNativeSessions,
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
