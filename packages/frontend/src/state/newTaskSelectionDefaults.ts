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

type SelectionStorage = Pick<Storage, "getItem" | "setItem">;

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
  return {
    projectId: firstValid(
      projects.map((project) => project.projectId),
      retained?.projectId,
      shellProjectId,
      defaults.projectId ?? undefined,
    ),
    agentId: firstValid(
      agents.map((agent) => agent.agentId),
      retained?.agentId,
      defaults.agentId ?? undefined,
    ),
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
  live: NewTaskContextIds,
): NewTaskContextIds {
  const stored = readRetainedNewTaskContext(
    snapshot.stateRoot.stateRootId,
    snapshot.client.clientInstanceId,
  );
  return {
    projectId: live.projectId ?? stored?.projectId,
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

function firstValid(available: string[], ...preferred: Array<string | undefined>) {
  return preferred.find((candidate) => candidate !== undefined && available.includes(candidate))
    ?? available[0];
}

function storageKey(stateRootId: string, clientInstanceId: string) {
  return `openaide.newTaskSelection:${stateRootId}:${clientInstanceId}`;
}

function availableSessionStorage(): Storage | undefined {
  try {
    return globalThis.sessionStorage;
  } catch {
    return undefined;
  }
}
