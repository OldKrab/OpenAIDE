import type {
  AgentSummary,
  ClientSnapshot,
  ClientInstanceId,
  NewTaskDefaultsSnapshot,
  StateRootId,
} from "@openaide/app-server-client";

import type { ProjectOption } from "./composerOptions";

export type NewTaskContextIds = {
  projectId?: string;
  agentId?: string;
};

export type InitialNewTaskProjectSelection = {
  projectId?: string;
  source: "shell" | "retained" | "app_server_default" | "fallback" | "none";
  shellProjectPresent: boolean;
  shellProjectValid: boolean;
  retainedProjectPresent: boolean;
  retainedProjectValid: boolean;
  defaultProjectPresent: boolean;
  defaultProjectValid: boolean;
};

export type LiveNewTaskContext = NewTaskContextIds & {
  workspaceRootsSeededProject?: boolean;
};

type SelectionStorage = Pick<Storage, "getItem" | "setItem">;

export type RetainedPreparedTaskLease = {
  preparationKey: string;
  taskId: string;
};

export function selectInitialNewTaskContext({
  retained,
  shellProjectId,
  defaults,
  projects,
  agents,
}: {
  retained?: NewTaskContextIds;
  shellProjectId?: string;
  defaults: NewTaskDefaultsSnapshot;
  projects: ProjectOption[];
  agents: AgentSummary[];
}): NewTaskContextIds {
  const project = selectInitialNewTaskProject({ retained, shellProjectId, defaults, projects });
  return {
    projectId: project.projectId,
    agentId: firstValid(
      agents.map((agent) => agent.agentId),
      retained?.agentId,
      defaults.agentId ?? undefined,
    ),
  };
}

/** Selects a Project and returns metadata-only evidence suitable for diagnostics. */
export function selectInitialNewTaskProject({
  retained,
  shellProjectId,
  defaults,
  projects,
}: {
  retained?: NewTaskContextIds;
  shellProjectId?: string;
  defaults: NewTaskDefaultsSnapshot;
  projects: ProjectOption[];
}): InitialNewTaskProjectSelection {
  const allProjectIds = projects.map((project) => project.projectId);
  const availableProjectIds = projects
    .filter((project) => project.available !== false)
    .map((project) => project.projectId);
  const retainedProjectId = retained?.projectId;
  const defaultProjectId = defaults.projectId ?? undefined;
  const shellProjectValid = shellProjectId !== undefined && allProjectIds.includes(shellProjectId);
  const retainedProjectValid = retainedProjectId !== undefined
    && (availableProjectIds.includes(retainedProjectId)
      || (availableProjectIds.length === 0 && allProjectIds.includes(retainedProjectId)));
  const defaultProjectValid = defaultProjectId !== undefined
    && availableProjectIds.includes(defaultProjectId);

  let projectId: string | undefined;
  let source: InitialNewTaskProjectSelection["source"] = "none";
  if (shellProjectValid) {
    projectId = shellProjectId;
    source = "shell";
  } else if (retainedProjectValid) {
    projectId = retainedProjectId;
    source = "retained";
  } else if (defaultProjectValid) {
    projectId = defaultProjectId;
    source = "app_server_default";
  } else if (availableProjectIds.length > 0) {
    projectId = availableProjectIds[0];
    source = "fallback";
  }

  return {
    projectId,
    source,
    shellProjectPresent: shellProjectId !== undefined,
    shellProjectValid,
    retainedProjectPresent: retainedProjectId !== undefined,
    retainedProjectValid,
    defaultProjectPresent: defaultProjectId !== undefined,
    defaultProjectValid,
  };
}

/** Reads one client's retained New Task choice within one App Server state root. */
export function readRetainedNewTaskContext(
  stateRootId: StateRootId | string,
  clientInstanceId: ClientInstanceId | string,
  storage: SelectionStorage | undefined = availableSessionStorage(),
): NewTaskContextIds | undefined {
  try {
    const value = storage?.getItem(storageKey(stateRootId, clientInstanceId));
    return value ? JSON.parse(value) as NewTaskContextIds : undefined;
  } catch {
    return undefined;
  }
}

/** Merges live selections over reload-retained choices for initialize reconciliation. */
export function retainedNewTaskContextForInitialization(
  snapshot: ClientSnapshot,
  live: LiveNewTaskContext,
): NewTaskContextIds {
  const stored = readRetainedNewTaskContext(
    snapshot.stateRoot.stateRootId,
    snapshot.client.clientInstanceId,
  );
  return {
    projectId: live.workspaceRootsSeededProject ? stored?.projectId : live.projectId ?? stored?.projectId,
    agentId: live.agentId ?? stored?.agentId,
  };
}

/** Retains selection locally; changing selectors never creates App Server preference traffic. */
export function retainNewTaskContext(
  stateRootId: StateRootId | string,
  clientInstanceId: ClientInstanceId | string,
  change: NewTaskContextIds,
  storage: SelectionStorage | undefined = availableSessionStorage(),
) {
  try {
    const current = readRetainedNewTaskContext(stateRootId, clientInstanceId, storage) ?? {};
    const next = {
      projectId: change.projectId ?? current.projectId,
      agentId: change.agentId ?? current.agentId,
    };
    storage?.setItem(storageKey(stateRootId, clientInstanceId), JSON.stringify(next));
  } catch {
    // A blocked session store only removes reload retention; live React state remains authoritative.
  }
}

/** Reads the leased Prepared Task identity needed to recover context changes across reload. */
export function readRetainedPreparedTaskLease(
  stateRootId: StateRootId | string,
  clientInstanceId: ClientInstanceId | string,
  storage: SelectionStorage | undefined = availableSessionStorage(),
): RetainedPreparedTaskLease | undefined {
  try {
    const value = storage?.getItem(preparedTaskLeaseStorageKey(stateRootId, clientInstanceId));
    return value ? JSON.parse(value) as RetainedPreparedTaskLease : undefined;
  } catch {
    return undefined;
  }
}

/** Retains only opaque lease identity; App Server remains authoritative for Prepared Task state. */
export function retainPreparedTaskLease(
  stateRootId: StateRootId | string,
  clientInstanceId: ClientInstanceId | string,
  lease: RetainedPreparedTaskLease,
  storage: SelectionStorage | undefined = availableSessionStorage(),
) {
  try {
    storage?.setItem(
      preparedTaskLeaseStorageKey(stateRootId, clientInstanceId),
      JSON.stringify(lease),
    );
  } catch {
    // A blocked session store removes reload recovery but never live lease ownership.
  }
}

function firstValid(available: string[], ...preferred: Array<string | undefined>) {
  return preferred.find((candidate) => candidate !== undefined && available.includes(candidate))
    ?? available[0];
}

function storageKey(stateRootId: string, clientInstanceId: string) {
  return `openaide.newTaskSelection:${stateRootId}:${clientInstanceId}`;
}

function preparedTaskLeaseStorageKey(stateRootId: string, clientInstanceId: string) {
  return `openaide.preparedTaskLease:${stateRootId}:${clientInstanceId}`;
}

function availableSessionStorage(): Storage | undefined {
  try {
    return globalThis.sessionStorage;
  } catch {
    return undefined;
  }
}
