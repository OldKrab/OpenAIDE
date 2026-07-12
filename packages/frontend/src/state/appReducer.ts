import type {
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
  TaskSnapshot,
  TaskSummary,
  ActivityToolDetails,
  ChatMessage,
} from "@openaide/app-shell-contracts";
import {
  selectionWithProject,
  selectionWithWorkspace,
  type ComposerAttachment,
  type ProjectOption,
  type WorkspaceRoot,
} from "./composerOptions";
import { reduceNewTaskState } from "./newTaskReducer";
import { applyAppServerReplica } from "./appServerReplicaState";
import { reduceSettingsState } from "./settingsReducer";
import {
  reconcileBackgroundTaskSnapshot,
  reconcileTaskSnapshotDependents,
  upsertTaskSummary,
} from "./taskSnapshotReconciliation";
import { reduceTaskInteractionState } from "./taskInteractionReducer";
import type { AppState, TaskChatScrollState } from "./store";

export type SnapshotIntent = "open" | "refresh";

type AppActionPayload =
  | { type: "appServer:error"; message: string }
  | { type: "appServer:ready" }
  | { type: "appServer:replica"; epoch: number; stateRootId: string }
  | { type: "tasks"; archived: boolean; tasks: TaskSummary[] }
  | { type: "tasks:error"; message: string }
  | { type: "task:list:remove"; taskId: string }
  | { type: "task:promoted"; snapshot: TaskSnapshot; activate: boolean }
  | { type: "snapshot"; snapshot: TaskSnapshot; intent: SnapshotIntent }
  | { type: "taskScroll:record"; taskId: string; scrollState: TaskChatScrollState }
  | { type: "prompt"; prompt: string }
  | { type: "projects"; projects: ProjectOption[]; initialProjectId?: string }
  | { type: "workspace:roots"; roots: WorkspaceRoot[] }
  | { type: "submit:start"; prompt?: string; context?: ComposerAttachment[] }
  | { type: "submit:cancel" }
  | { type: "submit:error"; message: string }
  | { type: "submit:attachments:invalidate"; taskId: string; message: string }
  | { type: "newTask:reset" }
  | { type: "newTask:prepared"; taskId: string }
  | { type: "newTask:agent"; agentId: string; agentLabel?: string; newTaskId?: string }
  | { type: "newTask:project"; project: ProjectOption; newTaskId?: string }
  | { type: "newTask:projectId"; projectId: string; newTaskId?: string }
  | { type: "newTask:isolation"; isolation: IsolationKind }
  | { type: "newTask:configOptions:start" }
  | { type: "newTask:configOptions:result"; catalog: ConfigOptionsCatalog }
  | { type: "newTask:configOptions:error"; message: string }
  | { type: "newTask:nativeSessions:start"; append: boolean }
  | { type: "newTask:nativeSessions:result"; result: AgentListSessionsResult; append: boolean }
  | { type: "newTask:nativeSessions:listError"; message: string }
  | { type: "newTask:nativeSessions:error"; sessionId: string; message: string }
  | { type: "newTask:nativeSessions:adopt"; sessionId: string }
  | { type: "newTask:nativeSessions:remove"; sessionId: string }
  | { type: "newTask:workspace"; workspace: WorkspaceRoot; newTaskId?: string }
  | { type: "newTask:attachment:add"; attachment: Attachment }
  | { type: "newTask:attachment:remove"; attachmentId: string }
  | { type: "taskInput:prompt"; taskId: string; prompt: string }
  | { type: "taskInput:attachment:add"; taskId: string; attachment: Attachment }
  | { type: "taskInput:attachment:addAppServer"; taskId: string; attachment: ComposerAttachment }
  | { type: "taskInput:attachment:remove"; taskId: string; attachmentId: string }
  | { type: "taskInput:clear"; taskId: string }
  | { type: "taskInput:submit"; taskId: string; input?: { prompt: string; context: ComposerAttachment[] } }
  | { type: "taskInput:sendError"; taskId: string; message?: string }
  | { type: "taskSend:accepted"; taskId: string; userMessageId: import("@openaide/app-server-client").MessageId }
  | { type: "taskInput:error"; taskId: string; message?: string }
  | { type: "taskInput:cancelError"; taskId: string; message: string }
  | { type: "taskInput:attachments:invalidate"; taskId: string; message: string }
  | { type: "taskOpen:start"; taskId: string }
  | { type: "taskOpen:error"; taskId: string; message: string }
  | { type: "chatPage:start"; taskId: string; requestGeneration: number }
  | { type: "chatPage:result"; taskId: string; requestGeneration: number; page: MessagePage }
  | { type: "chatPage:error"; taskId: string; requestGeneration: number; message: string }
  | { type: "toolDetail:start"; taskId: string; artifactId: string }
  | { type: "toolDetail:result"; taskId: string; artifactId: string; details: ActivityToolDetails }
  | { type: "toolDetail:error"; taskId: string; artifactId: string; message: string }
  | { type: "permission:responding"; requestId: string }
  | { type: "permission:error"; requestId: string; message: string }
  | { type: "appServerPermission:received"; requestId: string; message: ChatMessage; taskId?: string }
  | { type: "appServerPermission:resolved"; requestId: string }
  | { type: "question:responding"; requestId: string }
  | { type: "question:error"; requestId: string; message: string }
  | { type: "appServerQuestion:received"; requestId: string; message: ChatMessage; taskId?: string }
  | { type: "appServerQuestion:resolved"; requestId: string }
  | { type: "search:set"; query: string }
  | { type: "archive:set"; showArchived: boolean }
  | { type: "selection:set"; taskId: string }
  | { type: "selection:clear" }
  | { type: "settings:start" }
  | { type: "settings:sections"; tabs: SettingsTabId[] }
  | { type: "settings:agentDetailsResult"; generatedAt: string; agents: AgentSettingsRecord[] }
  | { type: "settings:mcpServersStart" }
  | { type: "settings:mcpServersResult"; generatedAt: string; availability: SettingsProjectionAvailability; servers: McpServerSettingsRecord[] }
  | { type: "settings:mcpServersError"; message: string }
  | { type: "settings:skillsStart" }
  | { type: "settings:skillsResult"; generatedAt: string; availability: SettingsProjectionAvailability; skills: SkillSettingsRecord[] }
  | { type: "settings:skillsError"; message: string }
  | { type: "settings:error"; message: string }
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
  | { type: "appServer:error" }
  | { type: "appServer:ready" }
  | { type: "appServer:replica" }
  | { type: "tasks:error" }
  | { type: "task:list:remove" }
  | { type: "task:promoted" }
  | { type: "snapshot" }
  | { type: "taskScroll:record" }
  | { type: "projects" }
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
    case "appServer:error":
    case "appServer:ready":
    case "appServer:replica":
    case "tasks:error":
    case "task:list:remove":
    case "task:promoted":
    case "snapshot":
    case "taskScroll:record":
    case "projects":
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
      const cacheKey = taskListCacheKey(action.archived);
      const taskListCache = {
        ...state.taskListCache,
        [cacheKey]: tasks,
      };
      if (state.showArchived !== action.archived) {
        return { ...state, taskListCache };
      }
      return {
        ...state,
        tasks,
        taskListCache,
        taskListError: undefined,
      };
    }
    case "tasks:error":
      return { ...state, taskListError: action.message };
    case "task:list:remove":
      {
        const cacheKey = taskListCacheKey(state.showArchived);
        const nextTasks = state.tasks.filter((task) => task.task_id !== action.taskId);
        const { [action.taskId]: _snapshot, ...taskSnapshots } = state.taskSnapshots;
        const { [action.taskId]: _snapshotEpoch, ...taskSnapshotReplicaEpochs } = state.taskSnapshotReplicaEpochs;
        const { [action.taskId]: _scrollState, ...taskChatScrollStates } = state.taskChatScrollStates;
        return {
          ...state,
          tasks: nextTasks,
          taskListCache: {
            ...state.taskListCache,
            [cacheKey]: nextTasks,
          },
          activeTaskId: state.activeTaskId === action.taskId ? undefined : state.activeTaskId,
          snapshot: state.snapshot?.task.task_id === action.taskId ? undefined : state.snapshot,
          taskSnapshots,
          taskSnapshotReplicaEpochs,
          taskChatScrollStates,
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
      const tasks = upsertTaskSummary(reconciled.tasks, action.snapshot.task);
      return {
        ...reconciled,
        tasks,
        taskListCache: {
          ...reconciled.taskListCache,
          [taskListCacheKey(reconciled.showArchived)]: tasks,
        },
      };
    }
    case "snapshot": {
      // New Task state belongs to the client-private New Task controller. It must not
      // enter visible Task navigation, active Task state, or normal Task caches.
      if (action.snapshot.lifecycle === "new") return state;
      const replicaEpoch = action.replicaEpoch ?? state.appServerReplicaEpoch;
      if (replicaEpoch < state.appServerReplicaEpoch) return state;
      if (action.intent === "refresh" && state.activeTaskId !== action.snapshot.task.task_id) {
        return reconcileBackgroundTaskSnapshot(state, action.snapshot, replicaEpoch);
      }
      const taskId = action.snapshot.task.task_id;
      const reconciliation = reconcileTaskSnapshotDependents(state, action.snapshot, replicaEpoch);
      if (reconciliation.state === state) return state;
      const { snapshot } = reconciliation;
      const tasks = upsertTaskSummary(state.tasks, snapshot.task);
      return {
        ...reconciliation.state,
        snapshot,
        showArchived: state.showArchived,
        searchQuery: action.intent === "open" ? "" : state.searchQuery,
        tasks,
        taskListCache: {
          ...state.taskListCache,
          [taskListCacheKey(state.showArchived)]: tasks,
        },
        activeTaskId: snapshot.task.task_id,
        taskOpenError: undefined,
        newTask: {
          ...state.newTask,
          configOptionsLoading: false,
          configOptionsError: undefined,
        },
      };
    }
    case "taskScroll:record":
      return {
        ...state,
        taskChatScrollStates: {
          ...state.taskChatScrollStates,
          [action.taskId]: action.scrollState,
        },
      };
    case "projects": {
      const selected = state.newTask.selection.projectId
        ? action.projects.find((project) => project.projectId === state.newTask.selection.projectId)
        : selectedProject(action.projects, action.initialProjectId);
      const selection = selected
        ? selectionWithProject(state.newTask.selection, selected)
        : state.newTask.selection;
      return {
        ...state,
        projects: action.projects,
        newTask: { ...state.newTask, selection },
      };
    }
    case "workspace:roots": {
      const firstRoot = action.roots[0];
      const selection =
        state.newTask.selection.workspaceRoot || !firstRoot
          ? state.newTask.selection
          : selectionWithWorkspace(state.newTask.selection, firstRoot);
      return { ...state, workspaceRoots: action.roots, workspaceRootsLoaded: true, newTask: { ...state.newTask, selection } };
    }
    case "search:set":
      return { ...state, searchQuery: action.query };
    case "archive:set":
      return {
        ...state,
        showArchived: action.showArchived,
        tasks: state.taskListCache[taskListCacheKey(action.showArchived)] ?? [],
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

function taskListCacheKey(showArchived: boolean) {
  return showArchived ? "archived" : "active";
}

function selectedProject(projects: ProjectOption[], initialProjectId: string | undefined) {
  return projects.find((project) => project.projectId === initialProjectId) ?? projects[0];
}
