import type { Dispatch } from "react";
import type {
  AgentListedSession,
  AppPreferencesRecord,
  CustomAgentCreateParams,
  CustomAgentMetadataUpdateParams,
  CustomAgentReplaceParams,
  PermissionDecision,
  ElicitationResponse,
  SettingsTabId,
} from "@openaide/app-shell-contracts";
import type { AppAction, SnapshotIntent } from "../state/appReducer";
import type { AgentOption, ComposerAttachment } from "../state/composerOptions";
import type { AppState } from "../state/store";
import type {
  AttachmentListDirectoryResult,
  FileBrowserEntryId,
  FileBrowserRoot,
  FileBrowserRootId,
  BackendConnection,
  TaskSnapshot as ProtocolTaskSnapshot,
  TaskId,
  WorkspaceBrowserRoot,
  WorkspaceListDirectoryResult,
} from "@openaide/app-server-client";

export type AppControllerCallbacks = {
  navigation: NavigationCallbacks;
  newTask: NewTaskCallbacks;
  settings: SettingsCallbacks;
  task: TaskCallbacks;
};

export type NavigationCallbacks = {
  archiveTask: (taskId: string) => void;
  changeSearch: (query: string) => void;
  loadNativeSessions: (cursor?: string) => void;
  openNativeSession: (session: AgentListedSession) => void;
  openNewTask: (projectId?: string) => void;
  openSettings: () => void;
  openTask: (taskId: string) => void;
  restoreTask: (taskId: string) => void;
  toggleArchived: () => void;
};

export type SettingsCallbacks = {
  authenticateAgent: (agentId: string, methodId: string) => void;
  createCustomAgent: (payload: CustomAgentCreateParams) => void;
  deleteCustomAgent: (agentId: string) => void;
  replaceCustomAgent: (payload: CustomAgentReplaceParams) => void;
  refreshSettings: () => void;
  selectSettingsTab: (tab: SettingsTabId) => void;
  setAcpTrace: (enabled: boolean) => void;
  setAgentEnabled: (agentId: string, enabled: boolean) => void;
  setComposerSubmitShortcut: (shortcut: AppPreferencesRecord["composer_submit_shortcut"]) => void;
  updateCustomAgentMetadata: (payload: CustomAgentMetadataUpdateParams) => void;
  unlockDeveloperSettings: () => void;
};

export type NewTaskCallbacks = {
  cancel: () => void;
  fileBrowser?: TaskFileBrowserCallbacks;
  resetOptionsRequestKey: () => void;
  selectConfigOption: (configId: string, value: string) => void;
  submit: (draft?: NewTaskDraftInput) => void;
  workspaceBrowser?: WorkspaceBrowserCallbacks;
};

export type NewTaskDraftInput = {
  prompt: string;
  context: ComposerAttachment[];
};

export type TaskCallbacks = {
  cancel: () => void;
  fileBrowser?: TaskFileBrowserCallbacks;
  loadChatPage: (beforeCursor: string) => void;
  loadToolDetail: (artifactId: string, refresh?: boolean) => void;
  revealAttachment: (attachmentId: string) => Promise<void>;
  removeAttachment: (attachmentId: string) => void;
  respondToPermission: (
    requestId: string,
    optionId: string,
    decision: PermissionDecision,
    source?: "agent" | "appServer",
  ) => void;
  respondToQuestion: (requestId: string, response: ElicitationResponse) => void;
  retryHistory: () => void;
  sendPrompt: (prompt?: string) => void;
  selectConfigOption: (configId: string, value: string) => void;
};

export type TaskFileBrowserCallbacks = {
  attachEmbedded: (entryId: FileBrowserEntryId) => Promise<void>;
  attachFileReference: (entryId: FileBrowserEntryId) => Promise<void>;
  attachPastedImage: (file: File, draft?: NewTaskDraftInput) => Promise<void>;
  listDirectory: (rootId: FileBrowserRootId, directoryId?: FileBrowserEntryId) => Promise<AttachmentListDirectoryResult>;
  listRoots: () => Promise<FileBrowserRoot[]>;
};

export type WorkspaceBrowserCallbacks = {
  listDirectory: (path: string) => Promise<WorkspaceListDirectoryResult>;
  listRoots: () => Promise<WorkspaceBrowserRoot[]>;
};

export type SnapshotRequestIdFactory = (taskId?: string, intent?: SnapshotIntent) => number;

export type PendingNewTaskPreparationResult = {
  taskId: TaskId;
  task?: ProtocolTaskSnapshot;
};

export type NewTaskStartAttempt = {
  cancelled: boolean;
  draft: NewTaskDraftInput;
  taskId?: TaskId;
};

export type AppCallbacksDependencies = {
  acceptTaskOpen?: (taskId: string, requestId: number | undefined, intent: SnapshotIntent) => boolean;
  backendConnection?: Pick<BackendConnection, "respond">
    & Partial<Pick<BackendConnection, "events" | "request">>;
  beginNavigationChange: (archived?: boolean) => number;
  createSnapshotRequestId: SnapshotRequestIdFactory;
  currentNavigationGeneration: () => number;
  dispatch: Dispatch<AppAction>;
  latestOptionsRequestKey: { current: string | undefined };
  newTaskStartAttempt: { current: NewTaskStartAttempt | undefined };
  pendingPreparedNewTask: (key: string) => Promise<PendingNewTaskPreparationResult> | undefined;
  requestNativeSessions: (cursor?: string, append?: boolean) => void;
  setAgents?: (agents: AgentOption[]) => void;
  setPreferences: (preferences: AppPreferencesRecord) => void;
  state: AppState;
};
