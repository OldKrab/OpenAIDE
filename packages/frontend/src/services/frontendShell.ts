import type {
  HostToWebviewMessage,
  SettingsTabId,
} from "@openaide/app-shell-contracts";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import type { PostHostMessage } from "../state/postHostMessage";
import type { WebTaskNotificationManager } from "../shells/webTaskNotifications";

export type FrontendShell = {
  bootstrap(): WebviewBootstrap;
  messages: {
    post: PostHostMessage;
    subscribe(listener: (message: HostToWebviewMessage) => void): () => void;
  };
  navigation: {
    openNewTask(projectId?: string): void;
    openSettings(): void;
    openTask(taskId: string, title?: string): void;
    replaceSettingsTab(tab: SettingsTabId): void;
    subscribe(listener: (bootstrap: WebviewBootstrap) => void): () => void;
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
