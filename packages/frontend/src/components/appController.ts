import { useCallback, useEffect, useMemo, useReducer, useRef, useState, useSyncExternalStore } from "react";
import type { Dispatch } from "react";
import type { AppPreferencesRecord, TaskSnapshot, TaskSummary } from "@openaide/app-shell-contracts";
import {
  getBackendConnection,
  getBootstrap,
  postHostMessage,
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
import {
  WORKTREE_CREATE,
  WORKTREE_REFRESH,
  WORKTREE_RECREATE,
  WORKTREE_RENAME,
  WORKTREE_REMOVE,
  WORKTREE_REMOVAL_PREFLIGHT,
  TASK_LIST,
  type ProjectId,
  type AgentId,
  type WorktreeId,
  type WorktreeOperationId,
  type WorktreeRepositorySnapshot,
  type WorktreeRepositoryId,
} from "@openaide/app-server-client";
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
import { mapProtocolTaskSummary } from "../state/appServerProtocolMapping";
import { useNativeSessionRouteLifecycle } from "./useNativeSessionRouteLifecycle";

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
  const [navigationFocusedTaskId, setNavigationFocusedTaskId] = useState<string | null | undefined>(
    initialBootstrap.surface === "invalid" ? undefined : initialBootstrap.focusedTaskId,
  );
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
    backendInitializationReady,
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
    setNavigationFocusedTaskId,
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
  useNativeSessionRouteLifecycle({
    asyncOperations: operationOwner,
    attachmentResources,
    backendConnection: backendConnectionRef,
    backendReady: backendInitializationReady,
    bootstrap,
    dispatch: newTaskDispatch,
    newTaskController,
    replicaEpoch,
    state,
  });
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
    navigationFocusedTaskId,
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
    setAgents,
    setPreferences,
    state: callbackState,
  });
  const automaticAuthRetry = useRef<string | undefined>(undefined);
  useEffect(() => {
    const preparation = newTaskSnapshot?.preparation;
    const selectedAgent = agents?.find((agent) => agent.id === state.newTask.selection.agentId);
    const retryKey = preparation?.kind === "blocked" && preparation.blocker.kind === "authRequired"
      && selectedAgent?.status === "connected"
      ? `${newTaskSnapshot?.task.task_id}:${selectedAgent.id}`
      : undefined;
    if (!retryKey || !selectedAgent || automaticAuthRetry.current === retryKey) return;
    automaticAuthRetry.current = retryKey;
    void callbacks.navigation.retryAgent(selectedAgent.id);
  }, [agents, callbacks.navigation, newTaskSnapshot, state.newTask.selection.agentId]);

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
  // A transport owns subscriptions and lifecycle cleanup, so create the shell default once per
  // mounted controller. Recreating it on every render causes initialization to cancel itself.
  const defaultBackendConnection = useMemo(() => getBackendConnection(), []);
  const backendConnection = options.backendConnection ?? defaultBackendConnection;
  const core = useAppControllerCore({ backendConnection });
  const request = backendConnection?.request;
  const { createSnapshotRequestId: _createSnapshotRequestId, dispatch, newTaskSnapshot, state, ...renderState } = core;
  const routedTaskId = state.snapshot?.task.task_id;
  const newTaskViewSnapshot = newTaskSnapshot ?? state.snapshot;
  const preparedTaskId = newTaskViewSnapshot?.task.has_messages === false
    ? newTaskViewSnapshot.task.task_id
    : undefined;
  const decoratedVisibleTasks = decorateTaskWorkspaces(renderState.visibleTasks, state);
  const decoratedActiveTask = renderState.activeTask
    ? decorateTaskWorkspaces([renderState.activeTask], state)[0]
    : undefined;
  const decoratedSnapshot = state.snapshot
    ? { ...state.snapshot, task: decorateTaskWorkspaces([state.snapshot.task], state)[0] }
    : undefined;
  const decoratedNewTaskSnapshot = newTaskViewSnapshot
    ? { ...newTaskViewSnapshot, task: decorateTaskWorkspaces([newTaskViewSnapshot.task], state)[0] }
    : undefined;
  const waitForWorktreeOperation = async (
    projectId: string,
    repositoryId: string,
    operationId: WorktreeOperationId,
    initial: WorktreeRepositorySnapshot,
    onProgress?: (operation: import("@openaide/app-server-client").WorktreeOperationSnapshot) => void,
  ) => {
    if (!request) throw new Error("App Server connection unavailable.");
    let repository = initial;
    const deadline = Date.now() + 120_000;
    while (Date.now() < deadline) {
      const operation = repository.operations?.find((candidate) => candidate.operationId === operationId);
      if (operation) onProgress?.(operation);
      if (operation?.state === "succeeded") return { operation, repository };
      if (operation?.state === "failed") throw new Error(operation.error ?? "Worktree operation failed.");
      await new Promise((resolve) => window.setTimeout(resolve, 250));
      const refreshed = await request(WORKTREE_REFRESH, {
        projectId: projectId as ProjectId,
        repositoryId: repositoryId as WorktreeRepositoryId,
      });
      repository = refreshed.repository;
      dispatch({ type: "worktreeRepository", repository });
    }
    throw new Error("Worktree operation did not finish. Refresh worktrees to check its state.");
  };
  const refreshWorktrees = async (project: import("../state/composerOptions").ProjectOption) => {
    if (!project.worktreeRepositoryId) throw new Error("This Project does not support worktrees.");
    if (!request) throw new Error("App Server connection unavailable.");
    const result = await request(WORKTREE_REFRESH, {
      projectId: project.projectId as ProjectId,
      repositoryId: project.worktreeRepositoryId as WorktreeRepositoryId,
    });
    dispatch({ type: "worktreeRepository", repository: result.repository });
  };

  return {
    ...renderState,
    activeTask: decoratedActiveTask,
    visibleTasks: decoratedVisibleTasks,
    taskNotifications: {
      stateRootId: state.appServerStateRootId,
      tasks: decorateTaskWorkspaces(state.taskLists.open ?? (state.showArchived ? [] : state.tasks), state),
    },
    intents: {
      newTask: {
        changePrompt: (prompt) => dispatch({ type: "prompt", prompt }),
        reportAttachmentError: (message) => dispatch({
          type: "submit:error",
          message: message ?? "Images can be attached after the Task is open.",
        }),
        selectAgent: (agentId, agentLabel) => dispatch({ type: "newTask:agent", agentId, agentLabel }),
        selectIsolation: (isolation) => dispatch({ type: "newTask:isolation", isolation }),
        selectProject: (project) => dispatch({ type: "newTask:project", project }),
        selectWorkspace: (workspace) => dispatch({ type: "newTask:workspace", workspace }),
        selectWorktree: (worktree) => dispatch({ type: "newTask:worktree", ...worktree }),
        refreshWorktrees,
        createWorktree: async (project, draft, onProgress) => {
          if (!project.worktreeRepositoryId) throw new Error("This Project does not support worktrees.");
          if (!request) throw new Error("App Server connection unavailable.");
          const result = await request(WORKTREE_CREATE, {
            projectId: project.projectId as ProjectId,
            repositoryId: project.worktreeRepositoryId as WorktreeRepositoryId,
            name: draft.name,
            base: draft.base,
            branch: draft.branch,
          });
          dispatch({ type: "worktreeRepository", repository: result.repository });
          const completed = await waitForWorktreeOperation(
            project.projectId,
            project.worktreeRepositoryId,
            result.operationId,
            result.repository,
            onProgress,
          );
          const created = completed.repository.worktrees.find(
            (worktree) => worktree.worktreeId === completed.operation.worktreeId,
          );
          if (!created) throw new Error("Created worktree is missing from the repository snapshot.");
          return created;
        },
        recreateWorktree: async (project, worktreeId, draft, onProgress) => {
          if (!project.worktreeRepositoryId) throw new Error("This Project does not support worktrees.");
          if (!request) throw new Error("App Server connection unavailable.");
          const result = await request(WORKTREE_RECREATE, {
            projectId: project.projectId as ProjectId,
            repositoryId: project.worktreeRepositoryId as WorktreeRepositoryId,
            worktreeId: worktreeId as WorktreeId,
            base: draft.base,
            branch: draft.branch,
          });
          dispatch({ type: "worktreeRepository", repository: result.repository });
          const completed = await waitForWorktreeOperation(
            project.projectId,
            project.worktreeRepositoryId,
            result.operationId,
            result.repository,
            onProgress,
          );
          const recreated = completed.repository.worktrees.find((worktree) => worktree.worktreeId === worktreeId);
          if (!recreated) throw new Error("Recreated worktree is missing from the repository snapshot.");
          return recreated;
        },
        removeWorktree: async (repositoryId, worktreeId) => {
          if (!request) throw new Error("App Server connection unavailable.");
          const project = state.projects.find((candidate) => candidate.worktreeRepositoryId === repositoryId);
          if (!project) throw new Error("Worktree Project is unavailable.");
          const result = await request(WORKTREE_REMOVE, {
            repositoryId: repositoryId as WorktreeRepositoryId,
            worktreeId: worktreeId as WorktreeId,
          });
          dispatch({ type: "worktreeRepository", repository: result.repository });
          await waitForWorktreeOperation(project.projectId, repositoryId, result.operationId, result.repository);
        },
        removalPreflight: async (repositoryId, worktreeId) => {
          if (!request) throw new Error("App Server connection unavailable.");
          const result = await request(WORKTREE_REMOVAL_PREFLIGHT, {
            repositoryId: repositoryId as WorktreeRepositoryId,
            worktreeId: worktreeId as WorktreeId,
          });
          return result.preflight;
        },
        renameWorktree: async (repositoryId, worktreeId, name) => {
          if (!request) throw new Error("App Server connection unavailable.");
          const result = await request(WORKTREE_RENAME, {
            repositoryId: repositoryId as WorktreeRepositoryId,
            worktreeId: worktreeId as WorktreeId,
            name,
          });
          dispatch({ type: "worktreeRepository", repository: result.repository });
        },
        openFolder: renderState.bootstrap.surface !== "invalid" && renderState.bootstrap.shell.kind !== "web"
          ? (repositoryId, worktreeId) => postHostMessage({
              type: "worktree.openFolder",
              payload: { repository_id: repositoryId, worktree_id: worktreeId },
            })
          : undefined,
        loadProjectTasks: async (projectId) => {
          if (!request) throw new Error("App Server connection unavailable.");
          const [active, archived] = await Promise.all([
            request(TASK_LIST, { lifecycle: "open", projectId: projectId as ProjectId }),
            request(TASK_LIST, { lifecycle: "archived", projectId: projectId as ProjectId }),
          ]);
          const context = {
            agents: renderState.agents?.map((agent) => ({
              agentId: agent.id as AgentId,
              label: agent.label,
              status: agent.enabled === false ? "disconnected" as const : "connected" as const,
            })),
            projects: state.projects.map((project) => ({
              projectId: project.projectId as ProjectId,
              label: project.label,
              workspaceRoot: project.workspaceRoot ?? "",
              available: project.available !== false,
              worktreeRepositoryId: project.worktreeRepositoryId as WorktreeRepositoryId | undefined,
              projectWorktreeId: project.projectWorktreeId as WorktreeId | undefined,
              worktreeError: project.worktreeError,
            })),
          };
          return [...active.tasks, ...archived.tasks].map((task) =>
            mapProtocolTaskSummary(task, Math.max(active.revision, archived.revision), context));
        },
        openTask: (taskId) => renderState.callbacks.navigation.openTask(taskId),
      },
      task: {
        refreshWorkspace: async () => {
          const task = decoratedSnapshot?.task ?? decoratedActiveTask;
          const project = task
            ? state.projects.find((candidate) => candidate.projectId === task.project_id)
            : undefined;
          if (!project?.worktreeRepositoryId) throw new Error("This Task has no worktree repository to refresh.");
          await refreshWorktrees(project);
        },
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
          tasks: state.tasks,
          worktreeRepositories: state.worktreeRepositories,
          snapshot: decoratedNewTaskSnapshot,
          workspaceRootsLoaded: state.workspaceRootsLoaded,
        },
        permissionResponses: state.permissionResponses,
        questionResponses: state.questionResponses,
        savedScrollState: routedTaskId ? state.taskChatScrollStates[routedTaskId] : undefined,
        snapshot: decoratedSnapshot,
        taskInput: routedTaskId ? state.taskInputs[routedTaskId] : undefined,
        taskOpenError: state.taskOpenError,
        toolDetails: state.toolDetails,
      },
      settings: state.settings,
    },
  };
}

function decorateTaskWorkspaces(tasks: TaskSummary[], state: AppState) {
  const worktrees = Object.values(state.worktreeRepositories).flatMap((repository) => repository.worktrees);
  return tasks.map((task) => {
    const worktree = task.worktree_id
      ? worktrees.find((candidate) => candidate.worktreeId === task.worktree_id)
      : undefined;
    if (!worktree) {
      if (task.worktree_id) return task;
      const project = state.projects.find((candidate) => candidate.projectId === task.project_id);
      return project ? {
        ...task,
        workspace_available: project.available ?? task.workspace_available,
        workspace_root: project.workspaceRoot ?? task.workspace_root,
      } : task;
    }
    return {
      ...task,
      worktree_name: worktree.name,
      git_ref: worktree.head.kind === "branch"
        ? worktree.head.name
        : `Detached · ${worktree.head.commit.slice(0, 7)}`,
      workspace_available: worktree.availability === "available",
      workspace_root: worktree.path,
    };
  });
}

/** @internal Prefer `useAppController`; this exposes reducer details for lifecycle tests only. */
export function useAppControllerTestHarness(options: AppControllerOptions = {}) {
  return useAppControllerCore(options);
}
