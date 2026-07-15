import { useCallback, useMemo, useReducer, useRef, useState, useSyncExternalStore } from "react";
import type { Dispatch } from "react";
import type { AppPreferencesRecord, TaskSnapshot, TaskSummary } from "@openaide/app-shell-contracts";
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
import type { NewTaskViewIntents, NewTaskViewState } from "./NewTaskView";
import type { TaskViewIntents } from "./TaskView";

/** Internal workflow assembly exposed only to the controller lifecycle tests. */
export type AppControllerTestHarness = {
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

export type AppControllerView = {
  appServerError?: string;
  navigation: {
    nativeSessions: AppState["newTask"]["nativeSessions"];
    newTaskSelection: AppState["newTask"]["selection"];
    projects: AppState["projects"];
    searchQuery: string;
    showArchived: boolean;
    taskListError?: string;
  };
  primaryTask: {
    chatPageState?: AppState["chatPages"][string];
    liveTextPresentation?: AppState["taskLiveTextPresentation"][string];
    newTask: NewTaskViewState;
    permissionResponses: AppState["permissionResponses"];
    questionResponses: AppState["questionResponses"];
    savedScrollState?: AppState["taskChatScrollStates"][string];
    snapshot?: TaskSnapshot;
    taskInput?: AppState["taskInputs"][string];
    taskOpenError?: AppState["taskOpenError"];
    toolDetails: AppState["toolDetails"];
  };
  settings: AppState["settings"];
};

/** Render-ready state and user-intent operations consumed by App surfaces. */
export type AppController = {
  activeTask?: TaskSummary;
  activeNavigationTaskId?: string;
  agents?: AgentOption[];
  backendReady: boolean;
  backendConnectionState: BackendConnectionState;
  bootstrap: WebviewBootstrap;
  callbacks: AppControllerCallbacks;
  intents: {
    newTask: NewTaskViewIntents;
    task: TaskViewIntents;
  };
  preferences: AppPreferencesRecord;
  retryTaskOpen: () => void;
  taskNotifications?: {
    stateRootId?: string;
    tasks: TaskSummary[];
  };
  view: AppControllerView;
  visibleTasks: AppState["tasks"];
};

export type AppControllerOptions = {
  backendConnection?: AppControllerBackendConnection;
};
function useAppControllerCore({ backendConnection }: AppControllerOptions = {}): AppControllerTestHarness {
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

export function useAppController(options: AppControllerOptions = {}): AppController {
  const core = useAppControllerCore(options);
  const { createSnapshotRequestId: _createSnapshotRequestId, dispatch, newTaskSnapshot, state, ...renderState } = core;
  const routedTaskId = state.snapshot?.task.task_id;
  const newTaskViewSnapshot = newTaskSnapshot ?? state.snapshot;
  const preparedTaskId = newTaskViewSnapshot?.task.has_messages === false
    ? newTaskViewSnapshot.task.task_id
    : undefined;

  return {
    ...renderState,
    taskNotifications: {
      stateRootId: state.appServerStateRootId,
      tasks: state.taskListCache.active ?? (state.showArchived ? [] : state.tasks),
    },
    intents: {
      newTask: {
        changePrompt: (prompt) => dispatch(preparedTaskId
          ? { type: "taskInput:prompt", taskId: preparedTaskId, prompt }
          : { type: "prompt", prompt }),
        reportAttachmentError: (message) => dispatch({
          type: "submit:error",
          message: message ?? "Images can be attached after the Task is open.",
        }),
        selectAgent: (agentId, agentLabel) => dispatch({ type: "newTask:agent", agentId, agentLabel }),
        selectIsolation: (isolation) => dispatch({ type: "newTask:isolation", isolation }),
        selectProject: (project) => dispatch({ type: "newTask:project", project }),
        selectWorkspace: (workspace) => dispatch({ type: "newTask:workspace", workspace }),
      },
      task: {
        changePrompt: (prompt) => {
          if (routedTaskId) dispatch({ type: "taskInput:prompt", taskId: routedTaskId, prompt });
        },
        recordScroll: (scrollState) => {
          if (routedTaskId) dispatch({ type: "taskScroll:record", taskId: routedTaskId, scrollState });
        },
        reportAttachmentError: (message) => {
          if (!routedTaskId) return;
          dispatch({
            type: "taskInput:error",
            taskId: routedTaskId,
            message: message ?? "Unable to attach image.",
          });
        },
      },
    },
    view: {
      appServerError: state.appServerError,
      navigation: {
        nativeSessions: state.newTask.nativeSessions,
        newTaskSelection: state.newTask.selection,
        projects: state.projects,
        searchQuery: state.searchQuery,
        showArchived: state.showArchived,
        taskListError: state.taskListError,
      },
      primaryTask: {
        chatPageState: routedTaskId ? state.chatPages[routedTaskId] : undefined,
        liveTextPresentation: routedTaskId ? state.taskLiveTextPresentation[routedTaskId] : undefined,
        newTask: {
          newTask: state.newTask,
          preparedTaskInput: preparedTaskId ? state.taskInputs[preparedTaskId] : undefined,
          projects: state.projects,
          snapshot: newTaskViewSnapshot,
          workspaceRootsLoaded: state.workspaceRootsLoaded,
        },
        permissionResponses: state.permissionResponses,
        questionResponses: state.questionResponses,
        savedScrollState: routedTaskId ? state.taskChatScrollStates[routedTaskId] : undefined,
        snapshot: state.snapshot,
        taskInput: routedTaskId ? state.taskInputs[routedTaskId] : undefined,
        taskOpenError: state.taskOpenError,
        toolDetails: state.toolDetails,
      },
      settings: state.settings,
    },
  };
}

/** @internal Prefer `useAppController`; this exposes reducer details for lifecycle tests only. */
export function useAppControllerTestHarness(options: AppControllerOptions = {}) {
  return useAppControllerCore(options);
}
