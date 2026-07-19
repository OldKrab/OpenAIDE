import type {
  HostToWebviewMessage,
  SettingsTabId,
} from "@openaide/app-shell-contracts";
import type { FrontendShell } from "../../../packages/frontend/src/services/frontendShell";
import openAideIconUrl from "../../vscode-extension/media/openaide.png";
import {
  createWebTaskNotificationManager,
  type WebTaskNotificationEnvironment,
} from "../../../packages/frontend/src/shells/webTaskNotifications";
import type { WebviewBootstrap } from "../../../packages/frontend/src/state/surfaceTypes";
import {
  appServerConnection,
  shellPreferences,
  subscribeWindowMessages,
} from "../../../packages/frontend/src/shells/domBootstrap";
import type { HostChannelMessage } from "../../../packages/frontend/src/state/postHostMessage";
import {
  createRuntimeLogger,
  safeWebviewTelemetryFields,
} from "../src/runtime-logger.mjs";

const WEB_ROUTE_EVENT = "openaide:webRoute";
const settingsTabs = new Set<SettingsTabId>(["agents", "mcp", "skills", "common"]);
const logger = createRuntimeLogger("openaide-webview");

/** Browser-history adapter owned by the Web App composition boundary. */
export function createWebAppShell(): FrontendShell {
  const bootstrap = () => webBootstrapForLocation();
  const post = (message: HostChannelMessage) => {
    switch (message.type) {
      case "webview.telemetry":
        logger.info("webview_telemetry", safeWebviewTelemetryFields(message.payload));
        return;
      case "secret.transaction.apply":
      case "secret.transaction.commit":
      case "secret.transaction.rollback":
        postMessage({
          type: "secret.transaction.result",
          payload: {
            requestId: message.payload.requestId,
            transactionId: message.payload.transactionId,
            ok: false,
            error: "Secure storage is unavailable in the Web App.",
          },
        });
        return;
      default:
        return;
    }
  };
  const navigate = (path: string) => {
    window.history.pushState(null, "", path);
    publishRoute();
  };
  const publishRoute = () => {
    window.dispatchEvent(new CustomEvent(WEB_ROUTE_EVENT, { detail: bootstrap() }));
  };
  const taskNotifications = createWebTaskNotificationManager(
    webTaskNotificationEnvironment((taskId) => navigate(`/task/${encodeURIComponent(taskId)}`)),
  );
  return {
    bootstrap,
    messages: { post, subscribe: subscribeWindowMessages },
    navigation: {
      openNewTask: (projectId) => navigate(newTaskPath(projectId)),
      openSettings: (agentId, returnToNewTask, projectId) => navigate(settingsPath(agentId, returnToNewTask, projectId)),
      openTask: (taskId) => navigate(`/task/${encodeURIComponent(taskId)}`),
      replaceSettingsTab(tab) {
        if (!isSettingsPath(window.location.pathname)) return;
        const next = `/settings?tab=${encodeURIComponent(tab)}`;
        if (`${window.location.pathname}${window.location.search}` === next) return;
        window.history.replaceState(null, "", next);
        publishRoute();
      },
      subscribe(listener) {
        const onRoute = (event: Event) => {
          const detail = event instanceof CustomEvent ? event.detail : undefined;
          listener(isWebviewBootstrap(detail) ? detail : bootstrap());
        };
        const onPopState = () => listener(bootstrap());
        window.addEventListener(WEB_ROUTE_EVENT, onRoute);
        window.addEventListener("popstate", onPopState);
        return () => {
          window.removeEventListener(WEB_ROUTE_EVENT, onRoute);
          window.removeEventListener("popstate", onPopState);
        };
      },
    },
    recovery: {
      openExternal: (url) => window.open(url, "_blank", "noopener,noreferrer"),
      reload: () => window.location.reload(),
    },
    taskNotifications,
  };
}

/** Adapts browser-profile storage, focus, permissions, and cross-tab messaging. */
function webTaskNotificationEnvironment(openTask: (taskId: string) => void): WebTaskNotificationEnvironment {
  const coordination = crossTabNotificationCoordination();
  return {
    storage: window.localStorage,
    notificationIconUrl: openAideIconUrl,
    now: () => Date.now(),
    isFocused: () => document.visibilityState === "visible"
      && typeof document.hasFocus === "function"
      && document.hasFocus(),
    notificationsSupported: () => typeof window.Notification === "function",
    notificationPermission: () => window.Notification?.permission ?? "default",
    requestNotificationPermission: () => window.Notification.requestPermission(),
    showNotification(title, options, onClick) {
      try {
        const notification = new window.Notification(title, options);
        notification.addEventListener("click", onClick);
        return { close: () => notification.close() };
      } catch (error) {
        logger.warn("desktop_notification_failed", {
          error_name: error instanceof Error ? error.name : "Error",
        });
        return { close: () => undefined };
      }
    },
    focusWindow: () => window.focus(),
    openTask: (taskId) => openTask(taskId),
    subscribeFocus(listener) {
      const heartbeat = typeof window.setInterval === "function"
        ? window.setInterval(listener, 10_000)
        : undefined;
      window.addEventListener("focus", listener);
      window.addEventListener("blur", listener);
      window.addEventListener("pagehide", listener);
      document.addEventListener?.("visibilitychange", listener);
      return () => {
        if (heartbeat !== undefined) window.clearInterval(heartbeat);
        window.removeEventListener("focus", listener);
        window.removeEventListener("blur", listener);
        window.removeEventListener("pagehide", listener);
        document.removeEventListener?.("visibilitychange", listener);
      };
    },
    publish: coordination.publish,
    subscribeMessages: coordination.subscribe,
  };
}

function crossTabNotificationCoordination() {
  const channel = typeof window.BroadcastChannel === "function"
    ? new window.BroadcastChannel("openaide.taskNotifications")
    : undefined;
  const storageKey = "openaide.desktopNotifications.message";
  const listeners = new Set<(message: unknown) => void>();
  const onMessage = (event: MessageEvent) => {
    for (const listener of listeners) listener(event.data);
  };
  const onStorage = (event: StorageEvent) => {
    if (event.key !== storageKey || !event.newValue) return;
    try {
      const envelope = JSON.parse(event.newValue) as { message?: unknown };
      for (const listener of listeners) listener(envelope.message);
    } catch {
      logger.warn("desktop_notification_coordination_invalid");
    }
  };
  channel?.addEventListener("message", onMessage);
  if (!channel) window.addEventListener("storage", onStorage);

  return {
    publish(message: unknown) {
      if (channel) {
        channel.postMessage(message);
        return;
      }
      try {
        window.localStorage.setItem(storageKey, JSON.stringify({
          nonce: `${Date.now()}-${Math.random()}`,
          message,
        }));
      } catch (error) {
        logger.warn("desktop_notification_coordination_failed", {
          error_name: error instanceof Error ? error.name : "Error",
        });
      }
    },
    subscribe(listener: (message: unknown) => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
        if (listeners.size > 0) return;
        channel?.removeEventListener("message", onMessage);
        channel?.close();
        if (!channel) window.removeEventListener("storage", onStorage);
      };
    },
  };
}

function postMessage(message: HostToWebviewMessage) {
  window.setTimeout(() => window.dispatchEvent(new MessageEvent("message", { data: message })), 0);
}

function webBootstrapForLocation(): WebviewBootstrap {
  const pathname = window.location.pathname;
  const shared = {
    shell: { kind: "web", navigationMode: "project" } as const,
    preferences: shellPreferences(),
    appServerConnection: appServerConnection(),
  };
  if (isSettingsPath(pathname)) {
    const search = new URLSearchParams(window.location.search);
    return {
      surface: "settings",
      settingsTab: settingsTabFromSearch(),
      settingsAgentId: search.get("agentId") ?? undefined,
      returnToNewTask: search.get("returnToNewTask") === "true",
      projectId: search.get("projectId") ?? undefined,
      ...shared,
    };
  }
  if (pathname === "/archive" || pathname.startsWith("/archive/")) {
    return { surface: "navigation", archived: true, ...shared };
  }
  const taskMatch = /^\/task\/([^/]+)\/?$/.exec(pathname);
  if (taskMatch) return { surface: "task", taskId: decodeURIComponent(taskMatch[1]), ...shared };
  if (pathname === "/new-task" || pathname.startsWith("/new-task/")) {
    return {
      surface: "task",
      projectId: new URLSearchParams(window.location.search).get("projectId") ?? undefined,
      ...shared,
    };
  }
  return { surface: "task", ...shared };
}

function isSettingsPath(pathname: string) {
  return pathname === "/settings" || pathname.startsWith("/settings/");
}

function settingsTabFromSearch() {
  const tab = new URLSearchParams(window.location.search).get("tab");
  return typeof tab === "string" && settingsTabs.has(tab as SettingsTabId) ? tab as SettingsTabId : undefined;
}

function newTaskPath(projectId?: string) {
  return projectId ? `/new-task?projectId=${encodeURIComponent(projectId)}` : "/new-task";
}

function settingsPath(agentId?: string, returnToNewTask?: boolean, projectId?: string) {
  const search = new URLSearchParams();
  if (agentId) search.set("agentId", agentId);
  if (returnToNewTask) search.set("returnToNewTask", "true");
  if (projectId) search.set("projectId", projectId);
  const query = search.toString();
  return query ? `/settings?${query}` : "/settings";
}

function isWebviewBootstrap(value: unknown): value is WebviewBootstrap {
  if (!value || typeof value !== "object") return false;
  const candidate = value as { surface?: unknown; shell?: { kind?: unknown; navigationMode?: unknown } };
  const validSurface = candidate.surface === "navigation"
    || candidate.surface === "task"
    || candidate.surface === "settings";
  return validSurface
    && candidate.shell?.kind === "web"
    && candidate.shell.navigationMode === "project";
}
