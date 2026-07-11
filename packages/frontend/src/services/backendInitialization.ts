import type {
  ClientInstanceId,
  InitializeParams,
  RequestedSurface,
  ShellCapability,
} from "@openaide/app-server-client";
import type { WebviewBootstrap } from "../state/surfaceTypes";

const CLIENT_INSTANCE_ID_KEY = "openaide.clientInstanceId";
let memoryClientInstanceId: ClientInstanceId | undefined;

export function getClientInstanceId(storage = availableSessionStorage()): ClientInstanceId {
  const stored = readStoredClientInstanceId(storage);
  if (stored) return stored;

  const id = createClientInstanceId();
  memoryClientInstanceId = id;
  try {
    storage?.setItem(CLIENT_INSTANCE_ID_KEY, id);
  } catch {
    // sessionStorage may be present but blocked; memory identity still keeps this tab stable.
  }
  return id;
}

export function initializeParamsForBootstrap(
  bootstrap: WebviewBootstrap,
  clientInstanceId = getClientInstanceId(),
): InitializeParams {
  const shellKind = shellKindForBootstrap(bootstrap);
  return {
    clientInstanceId,
    shell: { kind: shellKind },
    requestedSurface: requestedSurfaceForBootstrap(bootstrap),
    capabilities: {
      protocol: [
        "requestResponses",
        "stableClientRequestIds",
        "resync",
        "permissionResponses",
        "questionResponses",
      ],
      shell: [
        "openExternal",
        "revealFile",
        "pickLocalFile",
        "openTerminal",
        ...(shellKind === "vscodeExtension" ? ["readSecret", "writeSecret"] as ShellCapability[] : []),
        "showNotification",
      ],
    },
  };
}

function shellKindForBootstrap(bootstrap: WebviewBootstrap) {
  return bootstrap.surface !== "invalid" && bootstrap.appServerConnection?.kind === "webProxy"
    ? "web"
    : "vscodeExtension";
}

function requestedSurfaceForBootstrap(bootstrap: WebviewBootstrap): RequestedSurface {
  switch (bootstrap.surface) {
    case "navigation":
      return { kind: "home" };
    case "settings":
      return { kind: "settings" };
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

function readStoredClientInstanceId(storage: Storage | undefined): ClientInstanceId | undefined {
  if (memoryClientInstanceId) return memoryClientInstanceId;
  try {
    const stored = storage?.getItem(CLIENT_INSTANCE_ID_KEY);
    return stored ? (stored as ClientInstanceId) : undefined;
  } catch {
    return undefined;
  }
}

function createClientInstanceId(): ClientInstanceId {
  const randomId = globalThis.crypto?.randomUUID?.();
  return (randomId ?? `client-${Date.now()}-${Math.random().toString(36).slice(2)}`) as ClientInstanceId;
}

function availableSessionStorage(): Storage | undefined {
  try {
    return globalThis.sessionStorage;
  } catch {
    return undefined;
  }
}

type RequestedSurfaceTaskId = Extract<RequestedSurface, { kind: "task" }>["taskId"];
type RequestedSurfaceProjectId = NonNullable<Extract<RequestedSurface, { kind: "newTask" }>["projectId"]>;
