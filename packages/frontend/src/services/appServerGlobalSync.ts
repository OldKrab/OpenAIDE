import type {
  AgentCollectionSnapshot,
  AppServerSession,
  DeviceCollectionSnapshot,
  GlobalStateChanges,
  NewTaskDefaultsSnapshot,
  ProjectCollectionSnapshot,
  SettingsSnapshot,
  TerminalCollectionSnapshot,
  WorktreeRepositorySnapshot,
} from "@openaide/app-server-client";
import type { Dispatch } from "react";
import type { AppAction } from "../state/appReducer";
import { agentSettingsStatusFromProtocol, agentSignInFlowFromProtocol } from "../intents/agentSettingsRecords";
import { applyProtocolAgents } from "../state/appServerAgents";
import type { AgentOption } from "../state/composerOptions";
import { mapProtocolAppPreferences } from "../state/appPreferencesMapping";
import { mapProtocolRuntimeSettings } from "../state/runtimeSettingsMapping";
import { mapSettingsSections } from "../state/settingsProjectionMapping";
import { selectInitialNewTaskContext, type NewTaskContextIds } from "../state/newTaskSelectionDefaults";
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
  removedWorktreeRepositories?: string[];
  settings?: SettingsSnapshot | null;
  newTaskDefaults?: NewTaskDefaultsSnapshot | null;
};

/** The settings areas a snapshot or change carries, as reducer actions; shared by baselines and `sync`. */
export function settingsActions(settings: SettingsSnapshot): AppAction[] {
  const actions: AppAction[] = [];
  if (settings.runtime) {
    actions.push({ type: "settings:runtimeSettings", settings: mapProtocolRuntimeSettings(settings.runtime) });
  }
  if (settings.sections.length) {
    actions.push({ type: "settings:sections", tabs: mapSettingsSections(settings.sections) });
  }
  if (settings.preferences) {
    actions.push({ type: "settings:preferences", preferences: mapProtocolAppPreferences(settings.preferences) });
  }
  return actions;
}

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
    /** Receives preferences the Frontend also mirrors outside the reducer. */
    setPreferences?: (preferences: Extract<AppAction, { type: "settings:preferences" }>["preferences"]) => void;
    /** The New Task choices the user holds now; a defaults change never overrides them. */
    liveNewTaskContext?: () => NewTaskContextIds;
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
  for (const repositoryId of areas.removedWorktreeRepositories ?? []) {
    dispatch({ type: "worktreeRepository:removed", repositoryId });
  }
  if (areas.settings) {
    for (const action of settingsActions(areas.settings)) {
      if (action.type === "settings:preferences") deps.setPreferences?.(action.preferences);
      dispatch(action);
    }
  }
  if (areas.newTaskDefaults) applyNewTaskDefaults(areas.newTaskDefaults, context, deps);
}

/**
 * New Task defaults seed a New Task that has no choice yet. Selection runs through the same
 * rule as initialization with the live choices retained, so a user's pick always wins.
 */
function applyNewTaskDefaults(
  defaults: NewTaskDefaultsSnapshot,
  context: StateSubscriptionMappingContext,
  deps: { dispatch: Dispatch<AppAction>; liveNewTaskContext?: () => NewTaskContextIds },
) {
  const live = deps.liveNewTaskContext?.() ?? {};
  const agents = context.agents ?? [];
  const projects = projectsAction({ projects: context.projects ?? [] });
  if (projects.type !== "projects") return;
  const chosen = selectInitialNewTaskContext({ retained: live, defaults, projects: projects.projects, agents });
  if (chosen.projectId && chosen.projectId !== live.projectId) {
    deps.dispatch({ ...projects, initialProjectId: chosen.projectId });
  }
  if (chosen.agentId && chosen.agentId !== live.agentId) {
    const agent = agents.find((candidate) => candidate.agentId === chosen.agentId);
    deps.dispatch({ type: "newTask:agent", agentId: chosen.agentId, agentLabel: agent?.label ?? chosen.agentId });
  }
}

/**
 * Maps `sync` notifications into presentation actions. The state itself arrives in the
 * `client/initialize` reply and each recovery baseline, which the controller applies.
 */
export function startGlobalSyncSubscription({
  backendConnection,
  context,
  currentAgentId,
  dispatch,
  liveNewTaskContext,
  setAgents,
  setPreferences,
}: {
  backendConnection: GlobalSyncConnection;
  context: StateSubscriptionMappingContext;
  currentAgentId?: () => string;
  dispatch: Dispatch<AppAction>;
  liveNewTaskContext?: () => NewTaskContextIds;
  setAgents?: (agents: AgentOption[]) => void;
  setPreferences?: (preferences: Extract<AppAction, { type: "settings:preferences" }>["preferences"]) => void;
}) {
  return backendConnection.handleNotification("sync", (changes: GlobalStateChanges) => {
    applyGlobalAreas(
      {
        projects: changes.projects,
        agents: changes.agents,
        devices: changes.devices,
        terminals: changes.terminals,
        worktrees: changes.worktrees,
        removedWorktreeRepositories: changes.removedWorktreeRepositories,
        settings: changes.settings,
        newTaskDefaults: changes.newTaskDefaults,
      },
      context,
      { currentAgentId, dispatch, liveNewTaskContext, setAgents, setPreferences },
    );
  });
}
