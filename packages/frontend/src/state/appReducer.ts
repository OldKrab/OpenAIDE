import type {
  AgentListedSession,
  AgentListSessionsResult,
  AppPreferencesRecord,
  Attachment,
  ConfigOptionsCatalog,
  IsolationKind,
  McpServerSettingsRecord,
  MessagePage,
  RuntimeSettingsResult,
  AgentSettingsRecord,
  SettingsProjectionAvailability,
  SettingsTabId,
  SkillSettingsRecord,
  TaskPermissionPolicy,
  TaskSnapshot,
  TaskSummary,
  ActivityToolDetails,
} from "@openaide/app-shell-contracts";
import { projectIdForWorkspaceRoot } from "@openaide/app-shell-contracts";
import {
  selectionWithProject,
  selectionWithWorkspace,
  type ComposerAttachment,
  type ProjectOption,
  type WorkspaceRoot,
} from "./composerOptions";
import { reduceNewTaskState } from "./newTaskReducer";
import { applyAppServerReplica } from "./appServerReplicaState";
import { reduceSettingsState, type SettingsAgentCollectionEntry } from "./settingsReducer";
import {
  reconcileBackgroundTaskSnapshot,
  reconcileTaskSnapshotDependents,
  replaceTaskSummary,
} from "./taskSnapshotReconciliation";
import { reduceTaskInteractionState } from "./taskInteractionReducer";
import type { AppState, NativeSessionsState, TaskChatScrollState, TaskOpenError } from "./store";

export type SnapshotIntent = "open" | "refresh";

type AppActionPayload =
  | { type: "appServer:error"; message: string }
  | { type: "appServer:ready" }
  | { type: "appServer:replica"; epoch: number; stateRootId: string }
  | { type: "tasks"; archived: boolean; tasks: TaskSummary[] }
  | {
      type: "taskNavigation";
      archived: boolean;
      tasks: TaskSummary[];
      sessions: AgentListedSession[];
      hasMoreProjectIds: string[];
      loadingProjectIds?: string[];
      refreshing: boolean;
      refreshError?: string;
      recoveryKind?: NativeSessionsState["recoveryKind"];
      recoveryAgentId?: string;
      recoveryAgentLabel?: string;
    }
  | {
      type: "nativeSessionArchive:start";
      agentId: string;
      sessionId: string;
      action: "archive" | "restore";
    }
  | {
      type: "nativeSessionArchive:complete";
      agentId: string;
      sessionId: string;
    }
  | {
      type: "nativeSessionArchive:error";
      agentId: string;
      sessionId: string;
      action: "archive" | "restore";
      message: string;
    }
  | { type: "nativeSessionFork:start"; key: string }
  | { type: "nativeSessionFork:complete"; key: string; closeWarning: boolean }
  | { type: "nativeSessionFork:error"; key: string; unknown: boolean; message: string }
  | { type: "nativeSessionFork:clear"; key: string }
  | { type: "tasks:error"; message: string }
  | { type: "task:list:remove"; taskId: string }
  | { type: "task:promoted"; snapshot: TaskSnapshot; activate: boolean }
  | {
      type: "snapshot";
      snapshot: TaskSnapshot;
      intent: SnapshotIntent;
      liveText?: { messageId: string; channel: "agent" | "thought"; eventCursor: string };
      confirmedPermissionPolicy?: TaskPermissionPolicy;
    }
  | { type: "taskScroll:record"; taskId: string; scrollState: TaskChatScrollState }
  | { type: "taskChat:liveText"; taskId: string; messageId: string; channel: "agent" | "thought"; eventCursor: string }
  | { type: "prompt"; prompt: string }
  | { type: "newTask:error:clear" }
  | { type: "projects"; projects: ProjectOption[]; initialProjectId?: string }
  | { type: "worktreeRepository"; repository: import("@openaide/app-server-client").WorktreeRepositorySnapshot }
  | { type: "workspace:roots"; roots: WorkspaceRoot[] }
  | { type: "submit:start"; prompt?: string; context?: ComposerAttachment[] }
  | { type: "submit:cancel" }
  | { type: "submit:error"; message: string }
  | { type: "submit:attachments:invalidate"; taskId: string; message: string }
  | { type: "newTask:reset" }
  | { type: "newTask:prepared"; taskId: string }
  | { type: "newTask:replaced"; staleTaskId: string; taskId: string }
  | { type: "newTask:leaseExpired"; taskId: string; message: string; configOptions?: ConfigOptionsCatalog }
  | { type: "newTask:agent"; agentId: string; agentLabel?: string; newTaskId?: string }
  | { type: "newTask:project"; project: ProjectOption; newTaskId?: string }
  | { type: "newTask:projectId"; projectId: string; newTaskId?: string }
  | { type: "newTask:isolation"; isolation: IsolationKind }
  | { type: "newTask:configOptions:start" }
  | { type: "newTask:configOptions:result"; catalog: ConfigOptionsCatalog }
  | { type: "newTask:configOptions:error"; message: string }
  | { type: "newTask:nativeSessions:start"; append: boolean }
  | { type: "newTask:nativeSessions:result"; result: AgentListSessionsResult; append: boolean }
  | { type: "newTask:nativeSessions:listError"; message: string; recoveryKind?: NativeSessionsState["recoveryKind"] }
  | {
      type: "newTask:nativeSessions:error";
      sessionId: string;
      kind?: "conflict" | "notFound";
      message: string;
      recoverable?: boolean;
    }
  | { type: "newTask:nativeSessions:adopt"; sessionId: string }
  | { type: "newTask:nativeSessions:remove"; sessionId: string }
  | { type: "newTask:workspace"; workspace: WorkspaceRoot; newTaskId?: string }
  | { type: "newTask:worktree"; worktreeId?: string; label: string; path: string; newTaskId?: string }
  | { type: "newTask:attachment:add"; attachment: Attachment }
  | { type: "newTask:attachment:remove"; attachmentId: string }
  | { type: "taskInput:prompt"; taskId: string; prompt: string }
  | { type: "taskInput:attachment:add"; taskId: string; attachment: Attachment }
  | { type: "taskInput:attachment:addAppServer"; taskId: string; attachment: ComposerAttachment }
  | { type: "taskInput:attachment:remove"; taskId: string; attachmentId: string }
  | { type: "taskInput:clear"; taskId: string }
  | { type: "taskInput:submit"; taskId: string; input?: { prompt: string; context: ComposerAttachment[] } }
  | { type: "taskInput:sendError"; taskId: string; message?: string }
  | {
      type: "taskSend:accepted";
      taskId: string;
      userMessageId: import("@openaide/app-server-client").MessageId;
      /** Canonical Task state returned with an existing-Task Send acceptance. */
      snapshot?: TaskSnapshot;
    }
  | { type: "taskQueue:accepted"; taskId: string; queueRevision: number }
  | { type: "taskQueue:take:start"; taskId: string; item: import("@openaide/app-shell-contracts").QueuedMessage; index: number }
  | { type: "taskQueue:take:collapse"; taskId: string; queuedMessageId: string }
  | { type: "taskQueue:take:accepted"; taskId: string; queuedMessageId: string; prompt: string; context: ComposerAttachment[] }
  | { type: "taskQueue:take:error"; taskId: string; queuedMessageId: string; message: string }
  | { type: "taskInput:error"; taskId: string; message?: string }
  | { type: "taskInput:error:clear"; taskId: string }
  | { type: "taskConfig:result"; taskId: string; catalog: ConfigOptionsCatalog }
  | {
      type: "taskInput:configError";
      taskId: string;
      mutationId: string;
      message: string;
      catalog?: ConfigOptionsCatalog;
    }
  | { type: "taskInput:configError:clear"; taskId: string; mutationId: string }
  | { type: "taskInput:cancelError"; taskId: string; message: string }
  | { type: "taskInput:attachments:invalidate"; taskId: string; message: string }
  | { type: "taskOpen:start"; taskId: string }
  | { type: "taskOpen:error"; taskId: string; kind?: TaskOpenError["kind"]; message: string }
  | { type: "chatPage:start"; taskId: string; requestGeneration: number }
  | { type: "chatPage:result"; taskId: string; requestGeneration: number; page: MessagePage }
  | { type: "chatPage:error"; taskId: string; requestGeneration: number; message: string }
  | { type: "toolDetail:start"; taskId: string; artifactId: string }
  | { type: "toolDetail:result"; taskId: string; artifactId: string; details: ActivityToolDetails }
  | { type: "toolDetail:error"; taskId: string; artifactId: string; message: string }
  | { type: "permission:responding"; requestId: string }
  | { type: "permission:error"; requestId: string; message: string }
  | { type: "question:responding"; requestId: string }
  | { type: "question:error"; requestId: string; message: string }
  | { type: "search:set"; query: string }
  | { type: "archive:set"; showArchived: boolean }
  | { type: "selection:set"; taskId: string }
  | { type: "selection:clear" }
  | { type: "settings:start" }
  | { type: "settings:sections"; tabs: SettingsTabId[] }
  | { type: "settings:agentDetailsResult"; generatedAt: string; agents: AgentSettingsRecord[] }
  | { type: "settings:agentCollection"; agents: SettingsAgentCollectionEntry[] }
  | { type: "settings:mcpServersStart" }
  | { type: "settings:mcpServersResult"; generatedAt: string; availability: SettingsProjectionAvailability; servers: McpServerSettingsRecord[] }
  | { type: "settings:mcpServersError"; message: string }
  | { type: "settings:skillsStart" }
  | { type: "settings:skillsResult"; generatedAt: string; availability: SettingsProjectionAvailability; skills: SkillSettingsRecord[] }
  | { type: "settings:skillsError"; message: string }
  | { type: "settings:error"; message: string }
  | { type: "settings:error:clear" }
  | { type: "settings:agentSaved"; agentId: string; agent?: AgentSettingsRecord }
  | { type: "settings:agentReplaced"; oldAgentId: string; newAgentId: string; agent?: AgentSettingsRecord }
  | { type: "settings:agentUpdated"; agent: AgentSettingsRecord }
  | { type: "settings:agentDeleted"; agentId: string }
  | { type: "settings:preferences"; preferences: AppPreferencesRecord }
  | { type: "settings:developerAcpTrace"; enabled: boolean }
  | { type: "settings:runtimeSettings"; settings: RuntimeSettingsResult }
  | { type: "settings:tab"; tab: SettingsTabId };

export type AppAction = AppActionPayload & {
  /** Rejects results produced by an App Server process that has been replaced. */
  replicaEpoch?: number;
};

/** Binds App Server work to the process that started it so late outcomes are rejected. */
export function bindAppServerReplicaEpoch(
  dispatch: (action: AppAction) => void,
  replicaEpoch: number,
) {
  return (action: AppAction) => dispatch({ ...action, replicaEpoch } as AppAction);
}

type GlobalAction = Extract<
  AppAction,
  | { type: "tasks" }
  | { type: "taskNavigation" }
  | { type: "nativeSessionArchive:start" }
  | { type: "nativeSessionArchive:complete" }
  | { type: "nativeSessionArchive:error" }
  | { type: "nativeSessionFork:start" }
  | { type: "nativeSessionFork:complete" }
  | { type: "nativeSessionFork:error" }
  | { type: "nativeSessionFork:clear" }
  | { type: "appServer:error" }
  | { type: "appServer:ready" }
  | { type: "appServer:replica" }
  | { type: "tasks:error" }
  | { type: "task:list:remove" }
  | { type: "task:promoted" }
  | { type: "snapshot" }
  | { type: "taskScroll:record" }
  | { type: "taskChat:liveText" }
  | { type: "projects" }
  | { type: "worktreeRepository" }
  | { type: "workspace:roots" }
  | { type: "search:set" }
  | { type: "archive:set" }
  | { type: "selection:set" }
  | { type: "selection:clear" }
>;

export function appReducer(state: AppState, action: AppAction): AppState {
  if (action.replicaEpoch !== undefined && action.replicaEpoch < state.appServerReplicaEpoch) {
    return state;
  }
  if (action.type === "taskSend:accepted" && action.snapshot) {
    // Apply the canonical Chat projection and Composer settlement as one
    // Frontend transition. This prevents an accepted User message from
    // disappearing from Composer before it is visible in Chat.
    const interactionState = reduceTaskInteractionState(state, action) ?? state;
    return reduceGlobalState(interactionState, {
      type: "snapshot",
      snapshot: action.snapshot,
      intent: "refresh",
      replicaEpoch: action.replicaEpoch,
    });
  }
  const domainState =
    reduceNewTaskState(state, action)
    ?? reduceTaskInteractionState(state, action)
    ?? reduceSettingsState(state, action);
  if (domainState) return domainState;

  if (!isGlobalAction(action)) return state;
  return reduceGlobalState(state, action);
}

function isGlobalAction(action: AppAction): action is GlobalAction {
  switch (action.type) {
    case "tasks":
    case "taskNavigation":
    case "nativeSessionArchive:start":
    case "nativeSessionArchive:complete":
    case "nativeSessionArchive:error":
    case "nativeSessionFork:start":
    case "nativeSessionFork:complete":
    case "nativeSessionFork:error":
    case "nativeSessionFork:clear":
    case "appServer:error":
    case "appServer:ready":
    case "appServer:replica":
    case "tasks:error":
    case "task:list:remove":
    case "task:promoted":
    case "snapshot":
    case "taskScroll:record":
    case "taskChat:liveText":
    case "projects":
    case "worktreeRepository":
    case "workspace:roots":
    case "search:set":
    case "archive:set":
    case "selection:set":
    case "selection:clear":
      return true;
    default:
      return false;
  }
}

function reduceGlobalState(state: AppState, action: GlobalAction): AppState {
  switch (action.type) {
    case "appServer:error":
      return { ...state, appServerError: action.message, taskListError: action.message };
    case "appServer:ready":
      return { ...state, appServerError: undefined };
    case "appServer:replica":
      return applyAppServerReplica(state, action.epoch, action.stateRootId);
    case "tasks": {
      const tasks = action.tasks;
      const listKey = taskListKey(action.archived);
      const taskLists = {
        ...state.taskLists,
        [listKey]: tasks,
      };
      if (state.showArchived !== action.archived) {
        return { ...state, taskLists };
      }
      return {
        ...state,
        tasks,
        taskLists,
        taskListError: undefined,
      };
    }
    case "taskNavigation": {
      const nextSessions = {
        ...(action.archived ? state.archivedNativeSessions : state.newTask.nativeSessions),
        items: action.sessions,
        hasMoreProjectIds: action.hasMoreProjectIds,
        loadingProjectIds: action.loadingProjectIds ?? [],
        loaded: true,
        loading: action.refreshing,
        nextCursor: undefined,
        error: action.refreshError,
        recoveryKind: action.recoveryKind,
        recoveryAgentId: action.recoveryAgentId,
        recoveryAgentLabel: action.recoveryAgentLabel,
      };
      if (action.archived) {
        return {
          ...state,
          archivedNativeSessions: nextSessions,
        };
      }
      return {
        ...state,
        newTask: {
          ...state.newTask,
          nativeSessions: nextSessions,
        },
      };
    }
    case "nativeSessionArchive:start": {
      const key = `${action.agentId}\u0000${action.sessionId}`;
      return {
        ...state,
        nativeSessionMutations: {
          ...state.nativeSessionMutations,
          [key]: { action: action.action, state: "pending" },
        },
      };
    }
    case "nativeSessionArchive:complete": {
      const key = `${action.agentId}\u0000${action.sessionId}`;
      const { [key]: _completed, ...nativeSessionMutations } = state.nativeSessionMutations;
      return { ...state, nativeSessionMutations };
    }
    case "nativeSessionArchive:error": {
      const key = `${action.agentId}\u0000${action.sessionId}`;
      return {
        ...state,
        nativeSessionMutations: {
          ...state.nativeSessionMutations,
          [key]: { action: action.action, state: "failed", error: action.message },
        },
      };
    }
    case "nativeSessionFork:start":
      return {
        ...state,
        nativeSessionMutations: {
          ...state.nativeSessionMutations,
          [action.key]: { action: "fork", state: "pending" },
        },
      };
    case "nativeSessionFork:complete":
      return {
        ...state,
        nativeSessionMutations: {
          ...state.nativeSessionMutations,
          [action.key]: {
            action: "fork",
            state: "created",
            error: action.closeWarning ? "Fork created, but Agent resource cleanup failed." : undefined,
          },
        },
      };
    case "nativeSessionFork:error":
      return {
        ...state,
        nativeSessionMutations: {
          ...state.nativeSessionMutations,
          [action.key]: {
            action: "fork",
            state: action.unknown ? "unknown" : "failed",
            error: action.message,
          },
        },
      };
    case "nativeSessionFork:clear": {
      const { [action.key]: _cleared, ...nativeSessionMutations } = state.nativeSessionMutations;
      return { ...state, nativeSessionMutations };
    }
    case "tasks:error":
      return { ...state, taskListError: action.message };
    case "task:list:remove":
      {
        const listKey = taskListKey(state.showArchived);
        const nextTasks = state.tasks.filter((task) => task.task_id !== action.taskId);
        const { [action.taskId]: _snapshot, ...taskSnapshots } = state.taskSnapshots;
        const { [action.taskId]: _snapshotEpoch, ...taskSnapshotReplicaEpochs } = state.taskSnapshotReplicaEpochs;
        const { [action.taskId]: _scrollState, ...taskChatScrollStates } = state.taskChatScrollStates;
        const { [action.taskId]: _liveText, ...taskLiveTextPresentation } = state.taskLiveTextPresentation;
        return {
          ...state,
          tasks: nextTasks,
          taskLists: {
            ...state.taskLists,
            [listKey]: nextTasks,
          },
          activeTaskId: state.activeTaskId === action.taskId ? undefined : state.activeTaskId,
          snapshot: state.snapshot?.task.task_id === action.taskId ? undefined : state.snapshot,
          taskSnapshots,
          taskSnapshotReplicaEpochs,
          taskChatScrollStates,
          taskLiveTextPresentation,
        };
      }
    case "task:promoted": {
      if (action.activate) {
        return reduceGlobalState(state, {
          type: "snapshot",
          snapshot: action.snapshot,
          intent: "open",
          replicaEpoch: action.replicaEpoch,
        });
      }
      const replicaEpoch = action.replicaEpoch ?? state.appServerReplicaEpoch;
      const reconciled = reconcileBackgroundTaskSnapshot(state, action.snapshot, replicaEpoch);
      const tasks = replaceTaskSummary(reconciled.tasks, action.snapshot.task) ?? reconciled.tasks;
      return {
        ...reconciled,
        tasks,
        taskLists: {
          ...reconciled.taskLists,
          [taskListKey(reconciled.showArchived)]: tasks,
        },
      };
    }
    case "snapshot": {
      // New Task state belongs to the client-private New Task controller. It must not
      // enter visible Task navigation, active Task state, or normal Task caches.
      if (action.snapshot.lifecycle === "prepared") return state;
      const replicaEpoch = action.replicaEpoch ?? state.appServerReplicaEpoch;
      if (replicaEpoch < state.appServerReplicaEpoch) return state;
      if (action.intent === "refresh" && state.activeTaskId !== action.snapshot.task.task_id) {
        return settleTaskLiveTextPresentation(
          reconcileBackgroundTaskSnapshot(
            state,
            action.snapshot,
            replicaEpoch,
            action.confirmedPermissionPolicy,
          ),
          action.snapshot.task.task_id,
          action.snapshot.task.status,
        );
      }
      const taskId = action.snapshot.task.task_id;
      const reconciliation = reconcileTaskSnapshotDependents(
        state,
        action.snapshot,
        replicaEpoch,
        action.confirmedPermissionPolicy,
      );
      if (reconciliation.state === state) {
        const nextState = settleTaskLiveTextPresentation(state, taskId, action.snapshot.task.status);
        return action.liveText
          ? applyTaskLiveTextPresentation(state, taskId, action.liveText)
          : nextState;
      }
      const { snapshot } = reconciliation;
      const tasks = replaceTaskSummary(state.tasks, snapshot.task) ?? state.tasks;
      const nextState = {
        ...reconciliation.state,
        snapshot,
        showArchived: state.showArchived,
        searchQuery: action.intent === "open" ? "" : state.searchQuery,
        tasks,
        taskLists: {
          ...state.taskLists,
          [taskListKey(state.showArchived)]: tasks,
        },
        activeTaskId: snapshot.task.task_id,
        taskOpenError: undefined,
        newTask: {
          ...state.newTask,
          configOptionsLoading: false,
          configOptionsError: undefined,
        },
      };
      const presentationState = settleTaskLiveTextPresentation(nextState, taskId, snapshot.task.status);
      return action.liveText
        ? applyTaskLiveTextPresentation(nextState, taskId, action.liveText)
        : presentationState;
    }
    case "taskScroll:record":
      return {
        ...state,
        taskChatScrollStates: {
          ...state.taskChatScrollStates,
          [action.taskId]: action.scrollState,
        },
      };
    case "taskChat:liveText":
      return applyTaskLiveTextPresentation(state, action.taskId, action);
    case "projects": {
      const currentProject = !state.newTask.workspaceRootsSeededProject && state.newTask.selection.projectId
        ? action.projects.find((project) => project.projectId === state.newTask.selection.projectId)
        : undefined;
      const selected = currentProject ?? selectedProject(action.projects, action.initialProjectId);
      const selection = selected
        ? selectionWithProject(state.newTask.selection, selected)
        : {
            ...state.newTask.selection,
            projectId: undefined,
            workspaceRoot: "",
            workspaceLabel: "Workspace",
            worktreeId: undefined,
          };
      return {
        ...state,
        projects: action.projects,
        newTask: {
          ...state.newTask,
          error: undefined,
          selection,
          workspaceRootsSeededProject: undefined,
        },
      };
    }
    case "worktreeRepository":
      return {
        ...state,
        worktreeRepositories: {
          ...state.worktreeRepositories,
          [action.repository.repositoryId]: action.repository,
        },
      };
    case "workspace:roots": {
      const firstRoot = action.roots[0];
      const firstRootProjectId = firstRoot?.projectId
        ?? (firstRoot ? projectIdForWorkspaceRoot(firstRoot.path) : undefined);
      const fillsSelectedWorkspace = !state.newTask.selection.workspaceRoot
        && firstRoot !== undefined
        && (
          !state.newTask.selection.projectId
          || state.newTask.selection.projectId === firstRootProjectId
        );
      const seedsProject = fillsSelectedWorkspace && !state.newTask.selection.projectId;
      const selection = fillsSelectedWorkspace && firstRoot
        ? selectionWithWorkspace(state.newTask.selection, firstRoot)
        : state.newTask.selection;
      return {
        ...state,
        workspaceRoots: action.roots,
        workspaceRootsLoaded: true,
        newTask: {
          ...state.newTask,
          selection,
          workspaceRootsSeededProject: seedsProject
            ? true
            : state.newTask.workspaceRootsSeededProject,
        },
      };
    }
    case "search:set":
      return { ...state, searchQuery: action.query };
    case "archive:set":
      return {
        ...state,
        showArchived: action.showArchived,
        tasks: state.taskLists[taskListKey(action.showArchived)] ?? [],
        taskListError: undefined,
      };
    case "selection:set":
      return {
        ...state,
        activeTaskId: action.taskId,
        snapshot: state.snapshot?.task.task_id === action.taskId
          ? state.snapshot
          : state.taskSnapshots[action.taskId],
        taskOpenError: undefined,
        newTask: abandonNativeSessionOpening(state.newTask),
      };
    case "selection:clear":
      return {
        ...state,
        activeTaskId: undefined,
        snapshot: undefined,
        newTask: abandonNativeSessionOpening(state.newTask),
      };
  }
}

function applyTaskLiveTextPresentation(
  state: AppState,
  taskId: string,
  signal: { messageId: string; channel: "agent" | "thought"; eventCursor: string },
): AppState {
  return {
    ...state,
    taskLiveTextPresentation: {
      ...state.taskLiveTextPresentation,
      [taskId]: {
        ...state.taskLiveTextPresentation[taskId],
        [signal.channel]: {
          messageId: signal.messageId,
          eventCursor: signal.eventCursor,
        },
      },
    },
  };
}

function settleTaskLiveTextPresentation(
  state: AppState,
  taskId: string,
  status: TaskSnapshot["task"]["status"],
): AppState {
  if (taskAcceptsLiveText(status)) return state;
  if (!state.taskLiveTextPresentation[taskId]) return state;
  const { [taskId]: _settled, ...taskLiveTextPresentation } = state.taskLiveTextPresentation;
  return { ...state, taskLiveTextPresentation };
}

function taskAcceptsLiveText(status: TaskSnapshot["task"]["status"]) {
  return status === "active" || status === "waiting" || status === "stopping";
}

function abandonNativeSessionOpening(newTask: AppState["newTask"]): AppState["newTask"] {
  if (newTask.nativeSessions.adoptingSessionId === undefined) return newTask;
  // Navigation must remain local and immediate while the App Server finishes the superseded load.
  return {
    ...newTask,
    submitting: false,
    nativeSessions: {
      ...newTask.nativeSessions,
      adoptingSessionId: undefined,
    },
  };
}

function taskListKey(showArchived: boolean) {
  return showArchived ? "archived" : "open";
}

function selectedProject(projects: ProjectOption[], initialProjectId: string | undefined) {
  return projects.find((project) => project.projectId === initialProjectId) ?? projects[0];
}
