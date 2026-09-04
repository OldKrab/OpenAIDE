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
  loadingProjectIds?: string[];
  loading: boolean;
  loaded: boolean;
  nextCursor?: string;
  error?: string;
  adoptionError?: {
    sessionId: string;
    kind?: "conflict" | "notFound";
    message: string;
    recoverable?: boolean;
  };
  recoveryKind?: "nodeJsRequired" | "authRequired" | "setupRequired" | "launchFailed";
  recoveryAgentId?: string;
  recoveryAgentLabel?: string;
  adoptingSessionId?: string;
};

export type NativeSessionMutationState = {
  action: "archive" | "restore" | "fork";
  state: "pending" | "created" | "failed" | "unknown";
  error?: string;
};

export function nativeSessionMutationKey(agentId: string, sessionId: string) {
  return `${agentId}\u0000${sessionId}`;
}

export function taskForkMutationKey(taskId: string) {
  return `task:${taskId}`;
}

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
  /** Allows authoritative initialization to replace only the shell's automatic workspace seed. */
  workspaceRootsSeededProject?: boolean;
  configOptions?: ConfigOptionsCatalog;
  configOptionsLoading?: boolean;
  configOptionsError?: string;
  nativeSessions: NativeSessionsState;
  error?: string;
  /** True only when the error followed an accepted first-send attempt. */
  errorRetryable?: boolean;
};

export type TaskComposerInput = {
  prompt: string;
  context: ComposerAttachment[];
  /** Changes only when task/send accepts this Task's exact pending attempt. */
  acceptedUserMessageId?: MessageId;
  /** Changes only when task/queueAppend accepts this Task's exact pending attempt. */
  acceptedQueueRevision?: number;
  error?: string;
  configError?: {
    mutationId: string;
    message: string;
    catalogKey?: string;
  };
  pending?: PendingComposerSend;
  queueTake?: {
    item: import("@openaide/app-shell-contracts").QueuedMessage;
    index: number;
    stage: "pending" | "collapsing";
  };
  /** Refocuses Composer after one queued item becomes the ordinary draft. */
  acceptedQueueTakeId?: string;
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

export type TaskOpenError = {
  taskId: string;
  kind: "conflict" | "notFound" | "failed";
  message: string;
};

export type AppState = {
  appServerError?: string;
  appServerReplicaEpoch: number;
  appServerStateRootId?: string;
  tasks: TaskSummary[];
  taskLists: {
    open?: TaskSummary[];
    archived?: TaskSummary[];
  };
  archivedNativeSessions: NativeSessionsState;
  nativeSessionMutations: Record<string, NativeSessionMutationState>;
  taskListError?: string;
  activeTaskId?: string;
  snapshot?: TaskSnapshot;
  taskSnapshots: Record<string, TaskSnapshot>;
  taskSnapshotReplicaEpochs: Record<string, number>;
  taskChatScrollStates: Record<string, TaskChatScrollState>;
  taskLiveTextPresentation: Record<string, TaskLiveTextPresentation>;
  taskOpenError?: TaskOpenError;
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
    archivedNativeSessions: {
      items: [],
      loading: false,
      loaded: false,
    },
    nativeSessionMutations: {},
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
