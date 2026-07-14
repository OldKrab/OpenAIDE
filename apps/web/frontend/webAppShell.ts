import type {
  HostToWebviewMessage,
  SettingsTabId,
} from "@openaide/app-shell-contracts";
import type { FrontendShell } from "../../../packages/frontend/src/services/frontendShell";
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
  return {
    bootstrap,
    messages: { post, subscribe: subscribeWindowMessages },
    navigation: {
      openNewTask: (projectId) => navigate(newTaskPath(projectId)),
      openSettings: () => navigate("/settings"),
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
    return { surface: "settings", settingsTab: settingsTabFromSearch(), ...shared };
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
