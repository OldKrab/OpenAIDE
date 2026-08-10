import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CLIENT_DETACH, type AppServerSession } from "@openaide/app-server-client";
import type { HostToWebviewMessage, SettingsTabId } from "@openaide/app-shell-contracts";
import type { FrontendShell } from "../../../packages/frontend/src/services/frontendShell";
import { createShellAppearance } from "../../../packages/frontend/src/services/shellAppearance";
import type { PostHostMessage } from "../../../packages/frontend/src/state/postHostMessage";
import type { WebviewBootstrap } from "../../../packages/frontend/src/state/surfaceTypes";
import { quitDesktop } from "./desktopQuitLifecycle";

type DesktopBootstrap = {
  clientInstanceId: string;
  connection: {
    kind: "localHttp";
    endpointUrl: string;
    authToken: string;
  };
  platform: "linux" | "macos" | "windows";
};

type DesktopRoute =
  | { surface: "nativeSession"; agentId: string; nativeSessionId: string; projectId?: string }
  | { surface: "settings"; projectId?: string; settingsAgentId?: string; settingsTab?: SettingsTabId; returnToNewTask?: boolean }
  | { surface: "task"; projectId?: string; taskId?: string };

type DesktopCommand = "new-task" | "open-project" | "quit" | "settings";

/** Adapts the native validation host to the shared Frontend shell seam. */
export function createDesktopValidationShell(
  host: DesktopBootstrap,
  session: AppServerSession,
): FrontendShell {
  const nativeWindow = getCurrentWindow();
  const nativeWebview = getCurrentWebview();
  let backgroundSyncSequence = 0;
  let nativeZoomSequence = 0;
  let nativeZoomInFlight = false;
  let quitInFlight = false;
  let unzoomedViewport: ViewportSize | undefined;
  const syncNativeBackground = (theme: "light" | "dark") => {
    const operationId = `desktop-background-${++backgroundSyncSequence}`;
    const startedAt = performance.now();
    const color = theme === "dark" ? "#1f1f1f" : "#f8fafc";
    console.info(`desktop_window_background_sync_started operation_id=${operationId} theme=${theme}`);
    void Promise.all([
      nativeWindow.setBackgroundColor(color),
      nativeWebview.setBackgroundColor(color),
    ]).then(() => {
      console.info(
        `desktop_window_background_sync_completed operation_id=${operationId} outcome=success duration_ms=${Math.round(performance.now() - startedAt)}`,
      );
    }).catch(() => {
      console.warn(
        `desktop_window_background_sync_completed operation_id=${operationId} outcome=failure error_kind=native_background_sync duration_ms=${Math.round(performance.now() - startedAt)}`,
      );
    });
  };
  const appearance = createShellAppearance({
    body: document.body,
    onResolvedThemeChange: syncNativeBackground,
    storage: window.localStorage,
    storageKey: "dev.openaide.desktop-development.theme",
    systemTheme: window.matchMedia("(prefers-color-scheme: dark)"),
  });
  let route: DesktopRoute = { surface: "task" };
  const routeListeners = new Set<(bootstrap: WebviewBootstrap) => void>();
  const messageListeners = new Set<(message: HostToWebviewMessage) => void>();
  const publishRoute = () => {
    const next = routeBootstrap(route, host.clientInstanceId);
    for (const listener of routeListeners) listener(next);
  };
  const navigate = (next: DesktopRoute) => {
    route = next;
    publishRoute();
  };

  void listen<DesktopCommand>("desktop-command", ({ payload }) => {
    if (payload === "new-task") navigate({ surface: "task" });
    else if (payload === "settings") navigate({ surface: "settings" });
    else if (payload === "open-project") void pickProjectFolder();
    else if (payload === "quit" && !quitInFlight) {
      quitInFlight = true;
      const operationId = `desktop-quit-${crypto.randomUUID()}`;
      const startedAt = performance.now();
      console.info(`desktop_quit_started operation_id=${operationId}`);
      void quitDesktop({
        requestDetach: () => session.request(CLIENT_DETACH, {}),
        closeSession: () => session.close(),
        beforeExit: (detachOutcome) => {
          console.info(
            `desktop_quit_completed operation_id=${operationId} outcome=success detach_outcome=${detachOutcome} duration_ms=${Math.round(performance.now() - startedAt)}`,
          );
        },
        exitApp: () => invoke("complete_desktop_quit"),
      });
    }
  });

  const post: PostHostMessage = (message) => {
    if (message.type === "webview.telemetry") {
      void invoke("record_desktop_telemetry", { payload: message.payload });
    }
  };

  const pickProjectFolder = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== "string") return undefined;
    return { path: selected, label: folderLabel(selected) };
  };
  const toggleMaximize = async () => {
    if (host.platform !== "macos") {
      await nativeWindow.toggleMaximize();
      return;
    }
    if (nativeZoomInFlight) return;
    nativeZoomInFlight = true;
    const operationId = `desktop-native-zoom-${++nativeZoomSequence}`;
    const startedAt = performance.now();
    console.info(`desktop_native_zoom_started operation_id=${operationId}`);
    let presentation: Animation | undefined;
    try {
      const before = viewportSize();
      await invoke("prepare_desktop_native_zoom");
      const prepared = viewportSize();
      const expanding = prepared.width !== before.width || prepared.height !== before.height;
      if (expanding) unzoomedViewport = before;
      const target = expanding ? prepared : unzoomedViewport;

      // AppKit blocks WebKit's viewport updates during `zoom:`. Keep the real
      // native window animation, but let its already-composited root layer
      // visually follow the frame while WebKit is unable to run layout.
      presentation = target
        ? createNativeZoomPresentation(before, target, expanding)
        : undefined;
      await nextAnimationFrame();
      await nextAnimationFrame();
      presentation?.play();
      await invoke("perform_desktop_native_zoom");
      await nextAnimationFrame();
      presentation?.cancel();
      console.info(
        `desktop_native_zoom_completed operation_id=${operationId} outcome=success duration_ms=${Math.round(performance.now() - startedAt)}`,
      );
    } catch {
      presentation?.cancel();
      console.warn(
        `desktop_native_zoom_completed operation_id=${operationId} outcome=failure error_kind=native_zoom duration_ms=${Math.round(performance.now() - startedAt)}`,
      );
    } finally {
      nativeZoomInFlight = false;
    }
  };

  return {
    appearance,
    backendConnection: () => session,
    desktopWindow: {
      platform: host.platform,
      close: () => nativeWindow.close(),
      minimize: () => nativeWindow.minimize(),
      startDragging: () => nativeWindow.startDragging(),
      toggleMaximize,
    },
    bootstrap: () => routeBootstrap(route, host.clientInstanceId),
    messages: {
      post,
      subscribe(listener) {
        messageListeners.add(listener);
        return () => messageListeners.delete(listener);
      },
    },
    navigation: {
      openNewTask: (projectId) => navigate({ surface: "task", projectId }),
      openNativeSession: (agentId, nativeSessionId, projectId) => navigate({
        surface: "nativeSession",
        agentId,
        nativeSessionId,
        projectId,
      }),
      openSettings: (settingsAgentId, returnToNewTask, projectId, settingsTab) => navigate({
        surface: "settings",
        projectId,
        returnToNewTask,
        settingsAgentId,
        settingsTab,
      }),
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
    projects: { pickFolder: pickProjectFolder },
    recovery: {
      openExternal: (url) => { void openUrl(url); },
      reload: () => window.location.reload(),
    },
  };
}

function nextAnimationFrame() {
  return new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
}

type ViewportSize = { height: number; width: number };

function viewportSize(): ViewportSize {
  return { height: window.innerHeight, width: window.innerWidth };
}

function createNativeZoomPresentation(
  current: ViewportSize,
  target: ViewportSize,
  expanding: boolean,
) {
  const root = document.getElementById("root");
  if (!root || target.width <= 0 || target.height <= 0) return undefined;
  const compact = expanding ? current : target;
  const expanded = expanding ? target : current;
  const compactTransform = `scale(${compact.width / expanded.width}, ${compact.height / expanded.height})`;
  const compactFrame = { transform: compactTransform, transformOrigin: "top left" };
  const expandedFrame = { transform: "scale(1, 1)", transformOrigin: "top left" };
  const animation = root.animate(
    expanding
      ? [compactFrame, expandedFrame]
      : [expandedFrame, compactFrame],
    {
      duration: 260,
      easing: "cubic-bezier(0.42, 0, 0.58, 1)",
      fill: "both",
    },
  );
  animation.pause();
  animation.currentTime = 0;
  return animation;
}

function routeBootstrap(
  route: DesktopRoute,
  clientInstanceId: string,
): WebviewBootstrap {
  return {
    ...route,
    clientInstanceId,
    preferences: { composer_submit_shortcut: "mod_enter" },
    shell: { kind: "desktop", navigationMode: "project" },
  };
}

function folderLabel(selected: string) {
  const parts = selected.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts.at(-1) || selected;
}
