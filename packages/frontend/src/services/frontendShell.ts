import type {
  HostToWebviewMessage,
  SettingsTabId,
} from "@openaide/app-shell-contracts";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import type { PostHostMessage } from "../state/postHostMessage";
import type { WebTaskNotificationManager } from "../shells/webTaskNotifications";
import type { AppServerSession } from "@openaide/app-server-client";

export type FrontendShell = {
  bootstrap(): WebviewBootstrap;
  /** Supplies a shell-owned logical session when the renderer must not own transport. */
  backendConnection?: () => AppServerSession;
  messages: {
    post: PostHostMessage;
    subscribe(listener: (message: HostToWebviewMessage) => void): () => void;
  };
  navigation: {
    openNewTask(projectId?: string): void;
    openSettings(agentId?: string, returnToNewTask?: boolean, projectId?: string): void;
    openTask(taskId: string, title?: string): void;
    replaceSettingsTab(tab: SettingsTabId): void;
    subscribe(listener: (bootstrap: WebviewBootstrap) => void): () => void;
  };
  recovery: {
    /** Opens a trusted product-owned recovery URL outside the embedded surface. */
    openExternal(url: string): void;
    /** Reloads the owning shell when it exposes that recovery capability. */
    reload?: () => void;
  };
  /** Browser-profile notification integration; omitted by non-Web shells. */
  taskNotifications?: WebTaskNotificationManager;
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
