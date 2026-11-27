import type {
  AgentCollectionSnapshot,
  AppServerSession,
  DeviceCollectionSnapshot,
  GlobalStateChanges,
  ProjectCollectionSnapshot,
  TerminalCollectionSnapshot,
  WorktreeRepositorySnapshot,
} from "@openaide/app-server-client";
import type { Dispatch } from "react";
import type { AppAction } from "../state/appReducer";
import { agentSettingsStatusFromProtocol, agentSignInFlowFromProtocol } from "../intents/agentSettingsRecords";
import { applyProtocolAgents } from "../state/appServerAgents";
import type { AgentOption } from "../state/composerOptions";
import type { StateSubscriptionMappingContext } from "./appServerStateSubscriptions";
import { remappedTaskNavigationActions } from "./appServerStateSubscriptions";

type GlobalSyncConnection = Pick<AppServerSession, "handleNotification">;

/** The areas of the global state the Frontend renders live; each replaces its previous copy. */
export type GlobalAreas = {
  projects?: ProjectCollectionSnapshot | null;
  agents?: AgentCollectionSnapshot | null;
  devices?: DeviceCollectionSnapshot | null;
  terminals?: TerminalCollectionSnapshot | null;
  worktrees?: WorktreeRepositorySnapshot[];
};

export function projectsAction(projects: ProjectCollectionSnapshot): AppAction {
  return {
    type: "projects",
    projects: projects.projects.map((project) => ({
      projectId: project.projectId,
      label: project.label,
      workspaceRoot: project.workspaceRoot,
      available: project.available,
      worktreeRepositoryId: project.worktreeRepositoryId ?? undefined,
      projectWorktreeId: project.projectWorktreeId ?? undefined,
      worktreeError: project.worktreeError ?? undefined,
    })),
  };
}

export function agentCollectionAction(agents: AgentCollectionSnapshot): AppAction {
  return {
    type: "settings:agentCollection",
    agents: agents.agents.map((agent) => ({
      agentId: agent.agentId,
      status: agentSettingsStatusFromProtocol(agent.status),
      setupReason: agent.setupReason ?? undefined,
      signIn: agentSignInFlowFromProtocol(agent.signIn),
    })),
  };
}

/**
 * Applies the areas of one global-state change in one step: every action is dispatched before
 * control returns, so no render sees some areas new and others old.
 */
export function applyGlobalAreas(
  areas: GlobalAreas,
  context: StateSubscriptionMappingContext,
  deps: {
    currentAgentId?: () => string;
    dispatch: Dispatch<AppAction>;
    setAgents?: (agents: AgentOption[]) => void;
  },
) {
  const { dispatch } = deps;
  if (areas.projects) {
    context.projects = areas.projects.projects;
    dispatch(projectsAction(areas.projects));
  }
  if (areas.agents) {
    context.agents = areas.agents.agents;
    if (deps.setAgents) {
      applyProtocolAgents(areas.agents, deps.currentAgentId?.() ?? "", deps.setAgents, dispatch);
    }
    dispatch(agentCollectionAction(areas.agents));
  }
  if (areas.projects || areas.agents) {
    for (const action of remappedTaskNavigationActions(context)) dispatch(action);
  }
  if (areas.devices) dispatch({ type: "settings:devices", devices: areas.devices });
  if (areas.terminals) dispatch({ type: "terminals", terminals: areas.terminals });
  for (const repository of areas.worktrees ?? []) dispatch({ type: "worktreeRepository", repository });
}

/**
 * Maps `sync` notifications into presentation actions. The state itself arrives in the
 * `client/initialize` reply and each recovery baseline, which the controller applies.
 * TODO(state-sync phase 7): settings and New Task defaults changes from other clients are not
 * applied live (they were not before); apply them once the reducers keep in-progress edits.
 */
export function startGlobalSyncSubscription({
  backendConnection,
  context,
  currentAgentId,
  dispatch,
  setAgents,
}: {
  backendConnection: GlobalSyncConnection;
  context: StateSubscriptionMappingContext;
  currentAgentId?: () => string;
  dispatch: Dispatch<AppAction>;
  setAgents?: (agents: AgentOption[]) => void;
}) {
  return backendConnection.handleNotification("sync", (changes: GlobalStateChanges) => {
    applyGlobalAreas(
      {
        projects: changes.projects,
        agents: changes.agents,
        devices: changes.devices,
        terminals: changes.terminals,
        worktrees: changes.worktrees,
      },
      context,
      { currentAgentId, dispatch, setAgents },
    );
  });
}
