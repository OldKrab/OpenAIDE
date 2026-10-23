import type {
  HostToWebviewMessage,
  SettingsTabId,
} from "@openaide/app-shell-contracts";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import type { PostHostMessage } from "../state/postHostMessage";
import type { WebTaskNotificationManager } from "../shells/webTaskNotifications";
import type { AppServerSession } from "@openaide/app-server-client";
import type { PreSendAttachment } from "@openaide/app-server-client";

export type FileUploadProgress = { loaded: number; total: number };
export type AppThemePreference = "system" | "light" | "dark";

export type FrontendShellAppearance = {
  theme(): AppThemePreference;
  setTheme(theme: AppThemePreference): void;
};

export type DesktopWindowCapability = {
  platform: "linux" | "macos" | "windows";
  close(): Promise<void>;
  minimize(): Promise<void>;
  startDragging(): Promise<void>;
  toggleMaximize(): Promise<void>;
};

export type DesktopRuntimeEnvironment =
  | { kind: "native" }
  | { kind: "wsl"; distro: string };

export type DesktopRuntimeCapability = {
  snapshot(): {
    active: DesktopRuntimeEnvironment;
    wslDistros: string[];
  };
  select(environment: DesktopRuntimeEnvironment): Promise<void>;
};

export type DesktopUpdateKind =
  | "unavailable"
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "readyToUpdate"
  | "applying"
  | "failed";

export type DesktopUpdateSnapshot = {
  revision: number;
  installedVersion: string;
  kind: DesktopUpdateKind;
  unavailableReason?: "developmentBuild" | "notConfigured" | "unsupportedInstallation";
  offer?: {
    version: string;
    notes: string;
    sizeBytes: number;
    publishedAt?: string;
  };
  progress?: { downloadedBytes: number; totalBytes: number };
  error?:
    | "network"
    | "invalidManifest"
    | "untrustedArtifact"
    | "artifactTooLarge"
    | "insufficientSpace"
    | "downloadFailed"
    | "installFailed"
    | "configuration"
    | "unsupportedInstallation"
    | "incompleteUpdate"
    | "shutdownFailed";
  lastCheckedAtMs?: number;
  updatedVersion?: string;
};

/** Fixed native updater operations; callers cannot provide trust or transport configuration. */
export type DesktopUpdateCapability = {
  /**
   * How an update is applied: the shell restarts itself (default), or hands the
   * package to the operating system's installer, which closes the app.
   */
  installation?: "restart" | "systemInstaller";
  snapshot(): DesktopUpdateSnapshot;
  subscribe(listener: () => void): () => void;
  check(): Promise<void>;
  download(): Promise<void>;
  cancelDownload(): Promise<void>;
  restartAndUpdate(options?: { stopActiveWork?: boolean }): Promise<DesktopUpdateRestartResult>;
  openReleaseNotes(version: string): Promise<void>;
};

export type DesktopUpdateRestartResult = "started" | "activeWork" | "otherClients";

export type DesktopCommand = "check-for-updates" | "new-task" | "open-project" | "settings" | "switch-computer";

export type DesktopCommandCapability = {
  subscribe(listener: (command: DesktopCommand) => void): () => void;
};

export type SentFileOpenRequest = {
  taskId: string;
  messageId: string;
  attachmentIndex: number;
  label: string;
};

export type SentFileInteraction = {
  sentFileAction: "download" | "reveal";
  openSentFile(request: SentFileOpenRequest): void;
};

export type FileViewerDownloadResult = "started" | "notFound" | "permissionDenied" | "notAFile" | "unavailable";

/** The shell checks readability, then hands the current file to its download manager. */
export type FileViewerDownloads = {
  save(request: { handle: string; label: string; operationId: string }, signal: AbortSignal): Promise<FileViewerDownloadResult>;
};

export type FileViewerContentResult =
  | { kind: "bytes"; bytes: ArrayBuffer }
  | { kind: "tooLarge" | "notFound" | "permissionDenied" | "notAFile" | "unavailable" };

/** Reads the current file under the viewer handle's authority for an in-app preview.
 * The shell refuses a file above `maxBytes` before buffering it. */
export type FileViewerContent = {
  read(request: { handle: string; maxBytes: number; operationId: string }, signal: AbortSignal): Promise<FileViewerContentResult>;
};

export type FrontendFileAcquisition =
  | {
      kind: "webUpload";
      upload(
        taskId: string,
        file: File,
        onProgress: (progress: FileUploadProgress) => void,
        signal: AbortSignal,
      ): Promise<PreSendAttachment>;
    }
  | {
      kind: "nativePicker";
      pick(taskId: string): Promise<PreSendAttachment[]>;
    };

export type TerminalSocketCapability = {
  /** A `ws:` or `wss:` URL of the App Server's Terminal route in use now. */
  socketUrl(): string;
  /**
   * Hears the open-Terminal count as it changes, for a native Quit that warns without asking
   * the App Server. Desktop only.
   */
  openCountChanged?(count: number): void;
};

export type FrontendShell = {
  bootstrap(): WebviewBootstrap;
  connectionSettings?: import("./connectionSettings").ConnectionSettings;
  /** Writes text through the App Shell that owns system clipboard access. */
  clipboard?: {
    writeText(text: string): Promise<void>;
  };
  /** Shell-owned appearance; omitted when the embedding host owns theme selection. */
  appearance?: FrontendShellAppearance;
  /** Supplies a shell-owned logical session when the renderer must not own transport. */
  backendConnection?: () => AppServerSession;
  /** Native window operations exposed only by the Desktop shell. */
  desktopWindow?: DesktopWindowCapability;
  /** Desktop-owned backend OS selection, resolved before App Server startup. */
  desktopRuntime?: DesktopRuntimeCapability;
  /** Shell-owned list of other computers and the choice among them (ADR-0070). */
  computers?: import("./computers").ComputersCapability;
  /** Desktop-owned signed update lifecycle; omitted by other App Shells. */
  desktopUpdates?: DesktopUpdateCapability;
  /** Native menu and keyboard commands routed into the shared Desktop surface. */
  desktopCommands?: DesktopCommandCapability;
  messages: {
    post: PostHostMessage;
    subscribe(listener: (message: HostToWebviewMessage) => void): () => void;
  };
  navigation: {
    openNewTask(projectId?: string): void;
    /** Lets an embedding shell carry the selected Project across renderer instances. */
    retainNewTaskProject?(projectId: string): void;
    openNativeSession(agentId: string, nativeSessionId: string, projectId?: string): void;
    openSettings(agentId?: string, returnToNewTask?: boolean, projectId?: string, settingsTab?: SettingsTabId, settingsIntent?: import("@openaide/app-shell-contracts").SettingsIntent): void;
    openTask(taskId: string, title?: string, agentId?: string): void;
    closeSettings?(taskId?: string, projectId?: string): void;
    replaceSettingsTab(tab: SettingsTabId): void;
    /** Writes the open Agent into the Settings route so refresh stays on that detail. */
    replaceSettingsAgent?(agentId?: string): void;
    subscribe(listener: (bootstrap: WebviewBootstrap) => void): () => void;
  };
  recovery: {
    /** Opens a trusted product-owned recovery URL outside the embedded surface. */
    openExternal(url: string, message?: string): void;
    /** Reloads the owning shell when it exposes that recovery capability. */
    reload?: () => void;
  };
  /** Shell-owned recovery for environments where an empty workspace blocks Task creation. */
  workspace?: {
    openFolder(): void;
  };
  /** Native folder acquisition for adding a Project; Web renders its own explorer. */
  projects?: {
    pickFolder(): Promise<{ path: string; label: string } | undefined>;
  };
  /** Shell-specific acquisition; shared Frontend receives only opaque handles. */
  files?: FrontendFileAcquisition;
  /** The shell opens its camera for an image input with `capture`, so the composer offers Take photo. */
  photoCapture?: true;
  /** Opens a durable sent file using the host-native interaction. */
  sentFiles?: SentFileInteraction;
  /** Saves an App Server-owned export through a client-bound opaque handle. */
  supportExports?: {
    save(request: { fileHandleId: string; label: string }): Promise<"saved" | "cancelled">;
  };
  /** OS notifications for Task Attention Events, by the Web and Desktop shells; omitted elsewhere. */
  taskNotifications?: WebTaskNotificationManager;
  /**
   * Where the Frontend opens a Terminal byte stream (ADR-0071). The attach token in the
   * stream authenticates it, so the URL carries no credential. Omitted by shells that
   * open their own native terminal instead (VS Code).
   */
  terminals?: TerminalSocketCapability;
  /**
   * Opens the host's own terminal at a folder instead of the in-app Terminal page. VS Code only,
   * where the App Server runs on the extension host's computer.
   */
  nativeTerminal?: { open(folder: string): void };
  /** Opens Agent File References in the Task Panel File Viewer. Omitted by VS Code. */
  fileViewer?: true;
  /** Web-only download affordance; omitted shells keep their existing file-link behavior. */
  fileViewerDownloads?: FileViewerDownloads;
  /** Byte access for previews rendered by Frontend (PDF); omitted shells show the fallback. */
  fileViewerContent?: FileViewerContent;
  /**
   * A shell-served PDF renderer document. A shell whose page CSP would also bind a
   * `srcdoc` sandbox (Desktop) serves the renderer from its own origin under the renderer's
   * policy; omitted shells build the sandbox from the packaged script.
   */
  pdfRendererDocumentUrl?: string;
};

let installedShell: FrontendShell | undefined;

/** Installs the concrete App Shell before the shared Frontend is mounted. */
export function installFrontendShell(shell: FrontendShell): void {
  installedShell = shell;
}

export function frontendShell(): FrontendShell {
  if (!installedShell) throw new Error("Frontend App Shell was not installed.");
  return installedShell;
}

/** Returns the installed shell when optional shell capabilities are being probed. */
export function currentFrontendShell(): FrontendShell | undefined {
  return installedShell;
}
