import type {
  AgentListedSession,
  AgentSettingsRecord,
  ChatMessage,
  ConfigOptionsCatalog,
  McpServerSettingsRecord,
  RuntimeSettingsResult,
  SettingsProjectionAvailability,
  SettingsTabId,
  SkillSettingsRecord,
  TaskSnapshot,
  TaskSummary,
  ActivityToolDetails,
} from "@openaide/app-shell-contracts";
import {
  defaultSelection,
  type ComposerAttachment,
  type ComposerSelection,
  type ProjectOption,
  type WorkspaceRoot,
} from "./composerOptions";
import type { MessageId, WorktreeRepositorySnapshot } from "@openaide/app-server-client";

export type PendingComposerSend = {
  prompt: string;
  context: ComposerAttachment[];
  state: "sending";
};

export type NativeSessionsState = {
  items: AgentListedSession[];
  hasMoreProjectIds?: string[];
  loading: boolean;
  loaded: boolean;
  nextCursor?: string;
  error?: string;
  adoptionError?: { sessionId: string; message: string };
  recoveryKind?: "nodeJsRequired" | "authRequired" | "setupRequired" | "launchFailed";
  adoptingSessionId?: string;
};

export type NewTaskState = {
  prompt: string;
  question: string;
  submitting: boolean;
  context: ComposerAttachment[];
  pending?: {
    prompt: string;
    context: ComposerAttachment[];
    configOptions?: ConfigOptionsCatalog;
  };
  selection: ComposerSelection;
  configOptions?: ConfigOptionsCatalog;
  configOptionsLoading?: boolean;
  configOptionsError?: string;
  nativeSessions: NativeSessionsState;
  error?: string;
};

export type TaskComposerInput = {
  prompt: string;
  context: ComposerAttachment[];
  /** Changes only when task/send accepts this Task's exact pending attempt. */
  acceptedUserMessageId?: MessageId;
  error?: string;
  configError?: {
    mutationId: string;
    message: string;
    catalogKey?: string;
  };
  pending?: PendingComposerSend;
};

export type TaskChatScrollState = {
  ownership: "following" | "reading";
  scrollTop: number;
};

export type LiveTextPresentationSignal = {
  messageId: string;
  eventCursor: string;
};

export type TaskLiveTextPresentation = {
  agent?: LiveTextPresentationSignal;
  thought?: LiveTextPresentationSignal;
};

export type ChatPageState = {
  olderItems: ChatMessage[];
  hasBefore: boolean;
  startCursor?: string;
  /** Monotonic identity of the latest earlier-page request for this Task. */
  requestGeneration?: number;
  pending?: boolean;
  error?: string;
};

export type SettingsState = {
  activeTab: SettingsTabId;
  availableTabs?: SettingsTabId[];
  loading: boolean;
  runtimeSettings?: RuntimeSettingsResult;
  agentDetails?: AgentSettingsRecord[];
  agentDetailsGeneratedAt?: string;
  mcpServers?: McpServerSettingsRecord[];
  mcpServersAvailability?: SettingsProjectionAvailability;
  mcpServersGeneratedAt?: string;
  mcpServersLoading?: boolean;
  mcpServersError?: string;
  skills?: SkillSettingsRecord[];
  skillsAvailability?: SettingsProjectionAvailability;
  skillsGeneratedAt?: string;
  skillsLoading?: boolean;
  skillsError?: string;
  error?: string;
  savedAgentId?: string;
  deletedAgentId?: string;
};

export type ToolDetailState = {
  loading: boolean;
  details?: ActivityToolDetails;
  error?: string;
};

export function toolDetailCacheKey(taskId: string, artifactId: string) {
  return `${taskId}\u0000${artifactId}`;
}

export type AppState = {
  appServerError?: string;
  appServerReplicaEpoch: number;
  appServerStateRootId?: string;
  tasks: TaskSummary[];
  taskLists: {
    open?: TaskSummary[];
    archived?: TaskSummary[];
  };
  taskListError?: string;
  activeTaskId?: string;
  snapshot?: TaskSnapshot;
  taskSnapshots: Record<string, TaskSnapshot>;
  taskSnapshotReplicaEpochs: Record<string, number>;
  taskChatScrollStates: Record<string, TaskChatScrollState>;
  taskLiveTextPresentation: Record<string, TaskLiveTextPresentation>;
  taskOpenError?: { taskId: string; message: string };
  permissionResponses: Record<string, { responding: boolean; error?: string }>;
  questionResponses: Record<string, { responding: boolean; error?: string }>;
  searchQuery: string;
  showArchived: boolean;
  projects: ProjectOption[];
  worktreeRepositories: Record<string, WorktreeRepositorySnapshot>;
  workspaceRoots: WorkspaceRoot[];
  workspaceRootsLoaded: boolean;
  taskInputs: Record<string, TaskComposerInput>;
  chatPages: Record<string, ChatPageState>;
  toolDetails: Record<string, ToolDetailState>;
  settings: SettingsState;
  newTask: NewTaskState;
};

export const welcomeQuestions = [
  "What should the agent do?",
  "What needs to change?",
  "What should be fixed?",
  "What should be checked?",
  "Where should the agent start?",
];

export function createInitialState(): AppState {
  return {
    appServerReplicaEpoch: 0,
    tasks: [],
    taskLists: {},
    taskSnapshots: {},
    taskSnapshotReplicaEpochs: {},
    taskChatScrollStates: {},
    taskLiveTextPresentation: {},
    permissionResponses: {},
    questionResponses: {},
    searchQuery: "",
    showArchived: false,
    projects: [],
    worktreeRepositories: {},
    workspaceRoots: [],
    workspaceRootsLoaded: false,
    taskInputs: {},
    chatPages: {},
    toolDetails: {},
    settings: {
      activeTab: "agents",
      loading: false,
    },
    newTask: {
      prompt: "",
      question: pickQuestion(),
      submitting: false,
      context: [],
      selection: defaultSelection(),
      nativeSessions: {
        items: [],
        loading: false,
        loaded: false,
      },
    },
  };
}

export function pickQuestion(previous?: string) {
  let next = welcomeQuestions[Math.floor(Math.random() * welcomeQuestions.length)];
  if (welcomeQuestions.length > 1) {
    while (next === previous) {
      next = welcomeQuestions[Math.floor(Math.random() * welcomeQuestions.length)];
    }
  }
  return next;
}
