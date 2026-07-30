import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { openUrl } from "@tauri-apps/plugin-opener";
import type {
  HostToWebviewMessage,
  SettingsTabId,
} from "@openaide/app-shell-contracts";
import type { FrontendShell } from "../../../../packages/frontend/src/services/frontendShell";
import type { WebviewBootstrap } from "../../../../packages/frontend/src/state/surfaceTypes";
import type { HostChannelMessage } from "../../../../packages/frontend/src/state/postHostMessage";

type LocalHttpConnection = {
  kind: "localHttp";
  endpointUrl: string;
  authToken: string;
};

type DesktopRoute =
  | { surface: "task"; taskId?: string; projectId?: string }
  | { surface: "settings"; settingsTab?: SettingsTabId; projectId?: string };

type DesktopCommand =
  | "choose-folder"
  | "new-task"
  | "reload"
  | "settings"
  | "test-notification";

/** Adapts Tauri-native commands to the shared Frontend shell seam. */
export function createDesktopPrototypeShell(
  connection: LocalHttpConnection,
): FrontendShell {
  let route: DesktopRoute = { surface: "task" };
  const routeListeners = new Set<(bootstrap: WebviewBootstrap) => void>();
  const bootstrap = () => desktopBootstrap(route, connection);
  const publishRoute = () => {
    const next = bootstrap();
    for (const listener of routeListeners) listener(next);
  };
  const navigate = (next: DesktopRoute) => {
    route = next;
    publishRoute();
  };

  void listen<DesktopCommand>("desktop-prototype-command", ({ payload }) => {
    switch (payload) {
      case "new-task":
        navigate({ surface: "task" });
        return;
      case "settings":
        navigate({ surface: "settings" });
        return;
      case "reload":
        window.location.reload();
        return;
      case "choose-folder":
        void chooseFolder();
        return;
      case "test-notification":
        void showTestNotification();
        return;
    }
  });

  return {
    appearance: {
      setTheme: (theme) => getCurrentWindow().setTheme(theme === "system" ? null : theme),
    },
    bootstrap,
    messages: {
      post: (message) => handleHostMessage(message),
      subscribe(listener) {
        const onMessage = (event: MessageEvent<HostToWebviewMessage>) => {
          listener(event.data);
        };
        window.addEventListener("message", onMessage);
        return () => window.removeEventListener("message", onMessage);
      },
    },
    navigation: {
      openNewTask: (projectId) => navigate({ surface: "task", projectId }),
      openNativeSession: () => undefined,
      openSettings: (_agentId, _returnToNewTask, projectId, settingsTab) =>
        navigate({ surface: "settings", projectId, settingsTab }),
      openTask: (taskId) => navigate({ surface: "task", taskId }),
      replaceSettingsTab(settingsTab) {
        if (route.surface !== "settings") return;
        navigate({ ...route, settingsTab });
      },
      subscribe(listener) {
        routeListeners.add(listener);
        return () => routeListeners.delete(listener);
      },
    },
    recovery: {
      openExternal: (url) => void openUrl(url),
      reload: () => window.location.reload(),
    },
    workspace: {
      openFolder: () => void chooseFolder(),
    },
  };
}

function desktopBootstrap(
  route: DesktopRoute,
  connection: LocalHttpConnection,
): WebviewBootstrap {
  const shared = {
    shell: { kind: "desktop", navigationMode: "project" },
    appServerConnection: connection,
    preferences: { composer_submit_shortcut: "mod_enter", theme: "system" },
  } as const;
  return {
    ...route,
    ...shared,
  };
}

function handleHostMessage(message: HostChannelMessage) {
  if (message.type === "webview.telemetry") {
    console.info("desktop prototype telemetry", message.payload);
    return;
  }
  if (
    message.type === "secret.transaction.apply"
    || message.type === "secret.transaction.commit"
    || message.type === "secret.transaction.rollback"
  ) {
    console.warn("desktop prototype secure storage is intentionally not implemented");
    postToFrontend({
      type: "secret.transaction.result",
      payload: {
        requestId: message.payload.requestId,
        transactionId: message.payload.transactionId,
        ok: false,
        error: "Secure storage is outside this desktop spike.",
      },
    });
  }
}

function postToFrontend(message: HostToWebviewMessage) {
  window.setTimeout(() => {
    window.dispatchEvent(new MessageEvent("message", { data: message }));
  }, 0);
}

async function chooseFolder() {
  const selected = await open({ directory: true, multiple: false });
  await invoke("record_folder_picker_result", {
    selected: typeof selected === "string",
  });
}

async function showTestNotification() {
  let allowed = await isPermissionGranted();
  if (!allowed) allowed = (await requestPermission()) === "granted";
  if (!allowed) return;
  sendNotification({
    title: "OpenAIDE Desktop Prototype",
    body: "Native notifications are connected.",
  });
}
