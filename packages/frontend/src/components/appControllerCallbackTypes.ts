import type { Dispatch } from "react";
import type {
  AgentListedSession,
  AppPreferencesRecord,
  CustomAgentCreateParams,
  CustomAgentMetadataUpdateParams,
  CustomAgentReplaceParams,
  ConfigOptionCurrentValue,
  ElicitationResponse,
  SettingsTabId,
} from "@openaide/app-shell-contracts";
import type { AppAction, SnapshotIntent } from "../state/appReducer";
import type { AgentOption, ComposerAttachment } from "../state/composerOptions";
import type { AppState } from "../state/store";
import type {
  AttachmentListDirectoryResult,
  ClientInstanceId,
  FileBrowserEntryId,
  FileBrowserRoot,
  FileBrowserRootId,
  AppServerSession,
  BackendConnection,
  TaskSnapshot as ProtocolTaskSnapshot,
  TaskId,
  TaskSearchFilesResult,
  WorkspaceBrowserRoot,
  WorkspaceListDirectoryResult,
} from "@openaide/app-server-client";
import type { ComposerAttachmentResourceOwner } from "../services/attachmentResources";
import type { NewTaskController, NewTaskLease } from "./newTaskController";
import type { AsyncOperationOwner } from "../state/asyncOperationOwner";

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
  openSettings: (agentId?: string, returnToNewTask?: boolean, projectId?: string) => void;
  openTask: (taskId: string) => void;
  retryAgent: (agentId: string) => Promise<boolean>;
  restoreTask: (taskId: string) => void;
  toggleArchived: () => void;
};

export type SettingsCallbacks = {
  authenticateAgent: (agentId: string, methodId: string, values?: Record<string, string>) => Promise<boolean>;
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
  removeAttachment: (attachmentId: string) => void;
  selectConfigOption: (configId: string, value: ConfigOptionCurrentValue) => void;
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
  /** Starts one earlier-page request and returns its viewport/reducer generation. */
  loadChatPage: (beforeCursor: string) => number | undefined;
  /** Keeps full Tool details current until the disclosure closes or unmounts. */
  subscribeToolDetail: (artifactId: string) => () => void;
  revealAttachment: (attachmentId: string) => Promise<void>;
  removeAttachment: (attachmentId: string) => void;
  respondToPermission: (
    requestId: string,
    optionId: string,
  ) => void;
  respondToQuestion: (requestId: string, response: ElicitationResponse) => void;
  sendPrompt: (prompt?: string) => void;
  selectConfigOption: (configId: string, value: ConfigOptionCurrentValue) => void;
};

export type TaskFileBrowserCallbacks = {
  /** Logical Task/composer owner; callback objects may refresh while this stays stable. */
  ownerKey: string;
  attachEmbedded: (entryId: FileBrowserEntryId) => Promise<void>;
  attachFileReference: (entryId: FileBrowserEntryId) => Promise<void>;
  attachImage: (file: File, draft?: NewTaskDraftInput) => Promise<void>;
  attachmentMode?: "webUpload" | "nativePicker";
  attachFiles?: (
    files: File[],
    options: {
      onProgress: (progress: { loaded: number; total: number }) => void;
      signal: AbortSignal;
      maxFiles: number;
    },
  ) => Promise<void>;
  listDirectory: (rootId: FileBrowserRootId, directoryId?: FileBrowserEntryId) => Promise<AttachmentListDirectoryResult>;
  listRoots: () => Promise<FileBrowserRoot[]>;
  /** Searches the current Task Workspace and returns protocol-relative paths. */
  searchFiles: (query: string) => Promise<TaskSearchFilesResult>;
};

export type WorkspaceBrowserCallbacks = {
  /** Logical navigation owner for async listing settlement. */
  ownerKey: string;
  listDirectory: (path: string) => Promise<WorkspaceListDirectoryResult>;
  listRoots: () => Promise<WorkspaceBrowserRoot[]>;
};

export type SnapshotRequestIdFactory = (taskId?: string, intent?: SnapshotIntent) => number;

export type PendingNewTaskPreparationResult = {
  taskId: TaskId;
  task: ProtocolTaskSnapshot;
};

export type NewTaskStartAttempt = {
  cancelled: boolean;
  draft: NewTaskDraftInput;
  newTaskLease?: NewTaskLease;
  /** Defers cancellation until task/send has an authoritative outcome. */
  sendInFlight?: boolean;
  taskId?: TaskId;
};

export type AppCallbacksDependencies = {
  acceptTaskOpen?: (taskId: string, requestId: number | undefined, intent: SnapshotIntent) => boolean;
  attachmentResources?: ComposerAttachmentResourceOwner;
  backendConnection?: Partial<Pick<
    AppServerSession,
    "request" | "subscribeState"
  >>;
  asyncOperations: AsyncOperationOwner;
  clientInstanceId: ClientInstanceId | string;
  createSnapshotRequestId: SnapshotRequestIdFactory;
  dispatch: Dispatch<AppAction>;
  newTaskStartAttempt: { current: NewTaskStartAttempt | undefined };
  pendingPreparedNewTask: (key: string) => Promise<PendingNewTaskPreparationResult> | undefined;
  newTaskController?: NewTaskController;
  requestNativeSessions: (cursor?: string, append?: boolean, minimumSessionCount?: number) => void;
  setAgents?: (agents: AgentOption[]) => void;
  setPreferences: (preferences: AppPreferencesRecord) => void;
  state: AppState;
};
