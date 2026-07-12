import type {
  HostToWebviewMessage,
  SettingsTabId,
  WebviewAppServerConnection,
  WebviewToHostMessage,
} from "@openaide/app-shell-contracts";
import {
  createLocalHttpBackendConnection,
  createWebProxyBackendConnection,
} from "@openaide/app-server-client";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import { getClientInstanceId } from "./backendInitialization";
import { createStandaloneHost, standaloneBootstrap } from "./devHost";

declare global {
  interface Window {
    acquireVsCodeApi?: () => { postMessage: (message: unknown) => void };
  }
}

const WEB_ROUTE_EVENT = "openaide:webRoute";
const settingsTabs = new Set<SettingsTabId>(["agents", "mcp", "skills", "common"]);
const vscode = window.acquireVsCodeApi?.();
const standaloneHost = createStandaloneHost();
const webHost = createWebHost();

export function getBootstrap(): WebviewBootstrap {
  const appServerConnection = parseAppServerConnection(document.body.dataset.appServerConnection);
  const preferences = {
    composer_submit_shortcut:
      document.body.dataset.composerSubmitShortcut === "enter" ? "enter" : "mod_enter",
  } as const;
  if (document.body.dataset.shell === "web") {
    return webBootstrapForPath(window.location.pathname, appServerConnection, preferences);
  }
  const standalone = standaloneBootstrap();
  if (standalone) return standalone;
  const surface = document.body.dataset.surface;
  if (surface === "navigation" || surface === "settings" || surface === "task") {
    return {
      surface,
      taskId: document.body.dataset.taskId || undefined,
      projectId: document.body.dataset.projectId || undefined,
      preferences,
      appServerConnection,
    };
  }
  return {
    surface: "invalid",
  };
}

export function postHostMessage(message: WebviewToHostMessage) {
  if (vscode) {
    vscode.postMessage(message);
    return;
  }
  if (webHost) {
    webHost.postMessage(message);
    return;
  }
  standaloneHost?.postMessage(message);
}

export function updateWebSettingsTabRoute(tab: SettingsTabId) {
  if (typeof window === "undefined" || typeof document === "undefined" || document.body.dataset.shell !== "web") {
    return;
  }
  if (!isSettingsTab(tab) || !isSettingsPath(window.location.pathname)) return;
  const next = settingsPath(tab);
  if (`${window.location.pathname}${window.location.search}` === next) return;
  window.history.replaceState(null, "", next);
  window.dispatchEvent(new CustomEvent(WEB_ROUTE_EVENT, { detail: getBootstrap() }));
}

export function getBackendConnection() {
  const bootstrap = getBootstrap();
  if (bootstrap.surface !== "invalid" && bootstrap.appServerConnection?.kind === "localHttp") {
    return createLocalHttpBackendConnection({
      ...bootstrap.appServerConnection,
      connectionId: getClientInstanceId(),
    });
  }
  if (bootstrap.surface !== "invalid" && bootstrap.appServerConnection?.kind === "webProxy") {
    return createWebProxyBackendConnection({
      endpointUrl: bootstrap.appServerConnection.endpointUrl,
      connectionId: getClientInstanceId(),
    });
  }
  return undefined;
}

export function subscribeHostMessages(listener: (message: HostToWebviewMessage) => void) {
  const onMessage = (event: MessageEvent) => listener(event.data);
  window.addEventListener("message", onMessage);
  return () => window.removeEventListener("message", onMessage);
}

export function subscribeWebRouteChanges(listener: (bootstrap: WebviewBootstrap) => void) {
  if (typeof window === "undefined" || typeof document === "undefined" || document.body.dataset.shell !== "web") {
    return () => {};
  }
  const onRoute = (event: Event) => {
    const detail = event instanceof CustomEvent ? event.detail : undefined;
    listener(isWebviewBootstrap(detail) ? detail : getBootstrap());
  };
  const onPopState = () => listener(getBootstrap());
  window.addEventListener(WEB_ROUTE_EVENT, onRoute);
  window.addEventListener("popstate", onPopState);
  return () => {
    window.removeEventListener(WEB_ROUTE_EVENT, onRoute);
    window.removeEventListener("popstate", onPopState);
  };
}

function parseAppServerConnection(value: string | undefined): WebviewAppServerConnection | undefined {
  if (!value) return undefined;
  try {
    const parsed = JSON.parse(value) as unknown;
    if (!parsed || typeof parsed !== "object") return undefined;
    const record = parsed as Record<string, unknown>;
    return record.kind === "localHttp" &&
      typeof record.endpointUrl === "string" &&
      typeof record.authToken === "string"
      ? {
          kind: "localHttp",
          endpointUrl: record.endpointUrl,
          authToken: record.authToken,
        }
      : record.kind === "webProxy" && typeof record.endpointUrl === "string"
        ? {
            kind: "webProxy",
            endpointUrl: record.endpointUrl,
          }
      : undefined;
  } catch {
    return undefined;
  }
}

function createWebHost() {
  if (typeof window === "undefined" || typeof document === "undefined" || document.body.dataset.shell !== "web") {
    return undefined;
  }
  return {
    postMessage(message: WebviewToHostMessage) {
      switch (message.type) {
        case "secret.transaction.apply":
        case "secret.transaction.commit":
        case "secret.transaction.rollback":
          postWebHostMessage({
            type: "secret.transaction.result",
            payload: {
              requestId: message.payload.requestId,
              transactionId: message.payload.transactionId,
              ok: false,
              error: "Secure storage is unavailable in the Web App.",
            },
          });
          return;
        case "surface.openNewTask":
          navigateWeb(newTaskPath(message.payload?.project_id));
          return;
        case "surface.openArchive":
          navigateWeb(archivePath());
          return;
        case "surface.openSettings":
          navigateWeb(settingsPath());
          return;
        case "surface.openTask":
          navigateWeb(`/task/${encodeURIComponent(message.payload.task_id)}`);
          return;
        default:
          return;
      }
    },
  };
}

function postWebHostMessage(message: HostToWebviewMessage) {
  window.setTimeout(() => {
    window.dispatchEvent(new MessageEvent("message", { data: message }));
  }, 0);
}

function navigateWeb(path: string) {
  window.history.pushState(null, "", path);
  window.dispatchEvent(new CustomEvent(WEB_ROUTE_EVENT, { detail: getBootstrap() }));
}

function webBootstrapForPath(
  pathname: string,
  appServerConnection: WebviewAppServerConnection | undefined,
  preferences: Exclude<WebviewBootstrap["preferences"], undefined>,
): WebviewBootstrap {
  if (isSettingsPath(pathname)) {
    return {
      surface: "settings",
      settingsTab: settingsTabFromSearch(window.location.search),
      preferences,
      appServerConnection,
    };
  }
  if (isArchivePath(pathname)) {
    return {
      surface: "navigation",
      archived: true,
      preferences,
      appServerConnection,
    };
  }
  const taskMatch = /^\/task\/([^/]+)\/?$/.exec(pathname);
  if (taskMatch) {
    return {
      surface: "task",
      taskId: decodeURIComponent(taskMatch[1]),
      preferences,
      appServerConnection,
    };
  }
  if (pathname === "/new-task" || pathname.startsWith("/new-task/")) {
    return {
      surface: "task",
      projectId: new URLSearchParams(window.location.search).get("projectId") ?? undefined,
      preferences,
      appServerConnection,
    };
  }
  return { surface: "task", preferences, appServerConnection };
}

function isSettingsPath(pathname: string) {
  return pathname === "/settings" || pathname.startsWith("/settings/");
}

function isArchivePath(pathname: string) {
  return pathname === "/archive" || pathname.startsWith("/archive/");
}

function archivePath() {
  return "/archive";
}

function settingsPath(tab?: SettingsTabId) {
  return tab ? `/settings?tab=${encodeURIComponent(tab)}` : "/settings";
}

function settingsTabFromSearch(search: string) {
  const tab = new URLSearchParams(search).get("tab");
  return isSettingsTab(tab) ? tab : undefined;
}

function isSettingsTab(value: unknown): value is SettingsTabId {
  return typeof value === "string" && settingsTabs.has(value as SettingsTabId);
}

function newTaskPath(projectId: string | undefined) {
  return projectId ? `/new-task?projectId=${encodeURIComponent(projectId)}` : "/new-task";
}

function isWebviewBootstrap(value: unknown): value is WebviewBootstrap {
  if (!value || typeof value !== "object") return false;
  const surface = (value as { surface?: unknown }).surface;
  return surface === "navigation" || surface === "task" || surface === "settings" || surface === "invalid";
}
