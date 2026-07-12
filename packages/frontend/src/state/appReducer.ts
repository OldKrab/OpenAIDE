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
import { reduceSettingsState } from "./settingsReducer";
import {
  applyPendingInputReconciliation,
  pendingInputReconciliation,
  reconcileBackgroundTaskSnapshot,
  reconcileTaskNavigationTasks,
  shouldIgnoreStaleTaskSnapshot,
  upsertTaskSummary,
} from "./taskSnapshotReconciliation";
import { reduceTaskInteractionState } from "./taskInteractionReducer";
import { retainSnapshotWindow } from "./chatPageMerge";
import type { AppState } from "./store";

export type SnapshotIntent = "open" | "refresh";

export type AppAction =
  | { type: "appServer:error"; message: string }
  | { type: "appServer:ready" }
  | { type: "tasks"; tasks: TaskSummary[] }
  | { type: "tasks:error"; message: string }
  | { type: "task:list:remove"; taskId: string }
  | { type: "snapshot"; snapshot: TaskSnapshot; intent: SnapshotIntent }
  | { type: "taskScroll:record"; taskId: string; scrollTop: number }
  | { type: "prompt"; prompt: string }
  | { type: "projects"; projects: ProjectOption[]; activeProjectId?: string }
  | { type: "workspace:roots"; roots: WorkspaceRoot[] }
  | { type: "submit:start"; prompt?: string; context?: ComposerAttachment[] }
  | { type: "submit:cancel" }
  | { type: "submit:error"; message: string }
  | { type: "submit:attachments:invalidate"; taskId: string; message: string }
  | { type: "newTask:reset" }
  | { type: "newTask:prepared"; taskId: string }
  | { type: "newTask:agent"; agentId: string; agentLabel?: string }
  | { type: "newTask:project"; project: ProjectOption }
  | { type: "newTask:projectId"; projectId: string }
  | { type: "newTask:isolation"; isolation: IsolationKind }
  | { type: "newTask:configOptions:start" }
  | { type: "newTask:configOptions:result"; catalog: ConfigOptionsCatalog }
  | { type: "newTask:configOptions:error"; message: string }
  | { type: "newTask:nativeSessions:start"; append: boolean }
  | { type: "newTask:nativeSessions:result"; result: AgentListSessionsResult; append: boolean }
  | { type: "newTask:nativeSessions:error"; message: string }
  | { type: "newTask:nativeSessions:adopt"; sessionId: string }
  | { type: "newTask:nativeSessions:remove"; sessionId: string }
  | { type: "newTask:workspace"; workspace: WorkspaceRoot }
  | { type: "newTask:attachment:add"; attachment: Attachment }
  | { type: "newTask:attachment:remove"; attachmentId: string }
  | { type: "taskInput:prompt"; taskId: string; prompt: string }
  | { type: "taskInput:attachment:add"; taskId: string; attachment: Attachment }
  | { type: "taskInput:attachment:addAppServer"; taskId: string; attachment: ComposerAttachment }
  | { type: "taskInput:attachment:remove"; taskId: string; attachmentId: string }
  | { type: "taskInput:clear"; taskId: string }
  | { type: "taskInput:submit"; taskId: string; input?: { prompt: string; context: ComposerAttachment[] } }
  | { type: "taskInput:error"; taskId: string; message?: string }
  | { type: "taskInput:attachments:invalidate"; taskId: string; message: string }
  | { type: "taskOpen:start"; taskId: string }
  | { type: "taskOpen:error"; taskId: string; message: string }
  | { type: "chatPage:start"; taskId: string }
  | { type: "chatPage:result"; taskId: string; page: MessagePage }
  | { type: "chatPage:error"; taskId: string; message: string }
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

type GlobalAction = Extract<
  AppAction,
  | { type: "tasks" }
  | { type: "appServer:error" }
  | { type: "appServer:ready" }
  | { type: "tasks:error" }
  | { type: "task:list:remove" }
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
    case "tasks:error":
    case "task:list:remove":
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
    case "tasks": {
      const tasks = reconcileTaskNavigationTasks(state, action.tasks);
      return {
        ...state,
        tasks,
        taskListCache: {
          ...state.taskListCache,
          [taskListCacheKey(state.showArchived)]: tasks,
        },
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
        const { [action.taskId]: _scrollTop, ...taskScrollPositions } = state.taskScrollPositions;
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
          taskScrollPositions,
        };
      }
    case "snapshot": {
      if (action.intent === "refresh" && state.activeTaskId !== action.snapshot.task.task_id) {
        return reconcileBackgroundTaskSnapshot(state, action.snapshot);
      }
      const pendingReconciliation = pendingInputReconciliation(state, action.snapshot, {
        clearCommittedDraft: action.intent === "open",
      });
      if (shouldIgnoreStaleTaskSnapshot(state.snapshot, action.snapshot)) {
        return applyPendingInputReconciliation(state, action.snapshot.task.task_id, pendingReconciliation);
      }
      const hasMessages = action.snapshot.task.has_messages;
      const input = state.taskInputs[action.snapshot.task.task_id];
      const newTaskCommitted = pendingReconciliation.newTaskCommitted || (!state.newTask.pending && hasMessages);
      const tasks = upsertTaskSummary(state.tasks, action.snapshot.task);
      const terminalPermissionIds = terminalAppServerPermissionIds(action.snapshot);
      const appServerPermissionRequests = omitKeys(
        state.appServerPermissionRequests,
        terminalPermissionIds,
      );
      const permissionResponses = omitKeys(state.permissionResponses, terminalPermissionIds);
      const terminalQuestionIds = terminalAppServerQuestionIds(action.snapshot);
      const appServerQuestionRequests = omitKeys(state.appServerQuestionRequests, terminalQuestionIds);
      const questionResponses = omitKeys(state.questionResponses, terminalQuestionIds);
      const taskId = action.snapshot.task.task_id;
      const previousSnapshot = state.taskSnapshots[taskId];
      // A completed Native Session reconciliation is an authoritative replacement. Retaining
      // the old paging window here can resurrect rows the Agent no longer reports.
      const historyWasReconciled = (
        previousSnapshot?.history_sync.state === "checking"
        || previousSnapshot?.history_sync.state === "syncing"
      ) && previousSnapshot.chat.version !== action.snapshot.chat.version;
      const retainedChatPage = previousSnapshot && !historyWasReconciled
        ? retainSnapshotWindow(state.chatPages[taskId], previousSnapshot.chat, action.snapshot.chat)
        : undefined;
      return {
        ...state,
        snapshot: action.snapshot,
        showArchived: state.showArchived,
        searchQuery: action.intent === "open" ? "" : state.searchQuery,
        taskSnapshots: {
          ...state.taskSnapshots,
          [taskId]: action.snapshot,
        },
        chatPages: retainedChatPage
          ? { ...state.chatPages, [taskId]: retainedChatPage }
          : omitKeys(state.chatPages, new Set([taskId])),
        tasks,
        taskListCache: {
          ...state.taskListCache,
          [taskListCacheKey(state.showArchived)]: tasks,
        },
        activeTaskId: action.snapshot.task.task_id,
        taskOpenError: undefined,
        appServerPermissionRequests,
        permissionResponses,
        appServerQuestionRequests,
        questionResponses,
        taskInputs: input && (
          pendingReconciliation.taskInputCommitted
          || pendingReconciliation.taskInputRestoredSendCommitted
          || pendingReconciliation.taskInputDraftCommitted
        )
          ? {
              ...state.taskInputs,
              [action.snapshot.task.task_id]: { prompt: "", context: [] },
            }
          : state.taskInputs,
        newTask: {
          ...state.newTask,
          prompt: newTaskCommitted ? "" : state.newTask.prompt,
          context: newTaskCommitted ? [] : state.newTask.context,
          pending: newTaskCommitted ? undefined : state.newTask.pending,
          submitting: newTaskCommitted ? false : state.newTask.submitting,
          error: newTaskCommitted ? undefined : state.newTask.error,
          configOptionsLoading: false,
          configOptionsError: undefined,
          nativeSessions: { ...state.newTask.nativeSessions, adoptingSessionId: undefined, error: undefined },
        },
      };
    }
    case "taskScroll:record":
      return {
        ...state,
        taskScrollPositions: {
          ...state.taskScrollPositions,
          [action.taskId]: action.scrollTop,
        },
      };
    case "projects": {
      const selected = state.newTask.selection.projectId
        ? action.projects.find((project) => project.projectId === state.newTask.selection.projectId)
        : selectedProject(action.projects, action.activeProjectId);
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

function terminalAppServerPermissionIds(snapshot: TaskSnapshot) {
  return new Set(
    snapshot.chat.items.flatMap((item) => {
      const message = item.message;
      if (message.kind !== "permission") return [];
      if (message.state !== "resolved" && message.state !== "cancelled") return [];
      return [message.app_server_request_id, message.request_id].filter((id): id is string => Boolean(id));
    }),
  );
}

function terminalAppServerQuestionIds(snapshot: TaskSnapshot) {
  return new Set(
    snapshot.chat.items.flatMap((item) => {
      const message = item.message;
      if (message.kind !== "elicitation" || message.state === "pending") return [];
      return [message.app_server_request_id, message.request_id].filter((id): id is string => Boolean(id));
    }),
  );
}

function omitKeys<T>(record: Record<string, T>, keys: Set<string>) {
  if (!keys.size) return record;
  let changed = false;
  const next: Record<string, T> = {};
  for (const [key, value] of Object.entries(record)) {
    if (keys.has(key)) {
      changed = true;
      continue;
    }
    next[key] = value;
  }
  return changed ? next : record;
}

function selectedProject(projects: ProjectOption[], activeProjectId: string | undefined) {
  return projects.find((project) => project.projectId === activeProjectId) ?? projects[0];
}
