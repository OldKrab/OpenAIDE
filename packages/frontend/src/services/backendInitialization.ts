import type {
  ClientInstanceId,
  InitializeParams,
  ProjectId,
  RequestedSurface,
  ShellCapability,
  SubscriptionScope,
} from "@openaide/app-server-client";
import type { WebviewBootstrap } from "../state/surfaceTypes";

const CLIENT_INSTANCE_ID_KEY = "openaide.clientInstanceId";
const CLIENT_TAB_ID_KEY = "openaide.clientTabId";
const WINDOW_TAB_NAME_PREFIX = "openaide-tab:";
let memoryClientIdentity: {
  clientInstanceId: ClientInstanceId;
  owner?: object;
} | undefined;

type BrowserTabIdentityContext = {
  name: string;
  navigationType?: "back_forward" | "navigate" | "prerender" | "reload";
  owner?: object;
};

export function getClientInstanceId(
  storage = availableSessionStorage(),
  tab = availableBrowserTabIdentityContext(),
): ClientInstanceId {
  const owner = tab?.owner ?? tab;
  if (memoryClientIdentity && memoryClientIdentity.owner === owner) {
    return memoryClientIdentity.clientInstanceId;
  }

  const tabId = ensureBrowserTabId(tab);
  const stored = readStoredClientInstanceId(storage, tabId, tab?.navigationType);
  if (stored) {
    memoryClientIdentity = { clientInstanceId: stored, owner };
    return stored;
  }

  const id = createClientInstanceId();
  memoryClientIdentity = { clientInstanceId: id, owner };
  try {
    storage?.setItem(CLIENT_INSTANCE_ID_KEY, id);
    if (tabId) storage?.setItem(CLIENT_TAB_ID_KEY, tabId);
  } catch {
    // sessionStorage may be present but blocked; memory identity still keeps this tab stable.
  }
  return id;
}

export function clientInstanceIdForBootstrap(bootstrap: WebviewBootstrap): ClientInstanceId {
  if (bootstrap.surface !== "invalid" && bootstrap.clientInstanceId) {
    return bootstrap.clientInstanceId as ClientInstanceId;
  }
  return getClientInstanceId();
}

export function taskNavigationScopeForBootstrap(bootstrap: WebviewBootstrap): SubscriptionScope {
  const fixedProjectId = bootstrap.surface !== "invalid" && bootstrap.shell.navigationMode === "currentProject"
    ? bootstrap.projectId
    : undefined;
  return fixedProjectId
    ? { kind: "taskNavigation", projectId: fixedProjectId as ProjectId }
    : { kind: "taskNavigation" };
}

export function initializeParamsForBootstrap(
  bootstrap: WebviewBootstrap,
  clientInstanceId = clientInstanceIdForBootstrap(bootstrap),
): InitializeParams {
  const shellKind = bootstrap.surface === "invalid" ? "vscodeExtension" : bootstrap.shell.kind;
  return {
    clientInstanceId,
    shell: { kind: shellKind },
    requestedSurface: requestedSurfaceForBootstrap(bootstrap),
    capabilities: {
      // Permission and Question UI is replicated Task state. The browser
      // resolves it with pendingRequest/resolve, not reverse-RPC responses.
      protocol: [
        "requestResponses",
        "stableClientRequestIds",
        "resync",
      ],
      shell: [
        "openExternal",
        "revealFile",
        ...(shellKind === "vscodeExtension"
          ? ["pickLocalFile"] as ShellCapability[]
          : []),
        "openTerminal",
        ...(shellKind === "vscodeExtension"
          ? ["readSecret", "writeSecret"] as ShellCapability[]
          : []),
        "showNotification",
      ],
    },
  };
}

function requestedSurfaceForBootstrap(bootstrap: WebviewBootstrap): RequestedSurface {
  switch (bootstrap.surface) {
    case "navigation":
      return bootstrap.projectId
        ? { kind: "project", projectId: bootstrap.projectId as RequestedSurfaceProjectId }
        : { kind: "home" };
    case "settings":
      return { kind: "settings" };
    case "nativeSession":
      return { kind: "home" };
    case "task":
      return bootstrap.taskId
        ? { kind: "task", taskId: bootstrap.taskId as RequestedSurfaceTaskId }
        : {
            kind: "newTask",
            projectId: bootstrap.projectId ? (bootstrap.projectId as RequestedSurfaceProjectId) : undefined,
          };
    case "invalid":
      return { kind: "home" };
  }
}

function readStoredClientInstanceId(
  storage: Storage | undefined,
  tabId: string | undefined,
  navigationType: BrowserTabIdentityContext["navigationType"],
): ClientInstanceId | undefined {
  try {
    const stored = storage?.getItem(CLIENT_INSTANCE_ID_KEY);
    if (!stored) return undefined;
    if (tabId) {
      const storedTabId = storage?.getItem(CLIENT_TAB_ID_KEY);
      // A newly navigated document with copied sessionStorage is another tab.
      // Reload and history restoration keep the existing logical client.
      if (storedTabId !== tabId || navigationType === "navigate") return undefined;
    }
    return stored as ClientInstanceId;
  } catch {
    return undefined;
  }
}

function ensureBrowserTabId(tab: BrowserTabIdentityContext | undefined) {
  if (!tab) return undefined;
  const currentName = typeof tab.name === "string" ? tab.name : "";
  if (currentName.startsWith(WINDOW_TAB_NAME_PREFIX)) {
    return currentName.slice(WINDOW_TAB_NAME_PREFIX.length);
  }
  const tabId = createOpaqueId();
  tab.name = `${WINDOW_TAB_NAME_PREFIX}${tabId}`;
  return tabId;
}

function createClientInstanceId(): ClientInstanceId {
  return createOpaqueId() as ClientInstanceId;
}

function createOpaqueId() {
  return globalThis.crypto?.randomUUID?.()
    ?? `client-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function availableSessionStorage(): Storage | undefined {
  try {
    return globalThis.sessionStorage;
  } catch {
    return undefined;
  }
}

function availableBrowserTabIdentityContext(): BrowserTabIdentityContext | undefined {
  try {
    if (!globalThis.window) return undefined;
    const navigation = globalThis.performance?.getEntriesByType?.("navigation")[0] as
      | PerformanceNavigationTiming
      | undefined;
    return {
      get name() {
        return globalThis.window.name ?? "";
      },
      set name(value: string) {
        globalThis.window.name = value;
      },
      navigationType: navigation?.type,
      owner: globalThis.window,
    };
  } catch {
    return undefined;
  }
}

type RequestedSurfaceTaskId = Extract<RequestedSurface, { kind: "task" }>["taskId"];
type RequestedSurfaceProjectId = NonNullable<Extract<RequestedSurface, { kind: "newTask" }>["projectId"]>;
