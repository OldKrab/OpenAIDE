import type { ClientSnapshot } from "@openaide/app-server-client";
import type { AppAction } from "./appReducer";
import {
  mapProtocolTaskSnapshot,
  type ProtocolMappingWarning,
} from "./appServerProtocolMapping";
import { agentCollectionAction } from "../services/appServerGlobalSync";
import { mapProtocolAppPreferences } from "./appPreferencesMapping";
import { mapProtocolRuntimeSettings } from "./runtimeSettingsMapping";
import { mapSettingsSections } from "./settingsProjectionMapping";
import {
  selectInitialNewTaskContext,
  selectInitialNewTaskProject,
  type InitialNewTaskProjectSelection,
  type NewTaskContextIds,
} from "./newTaskSelectionDefaults";

export type InitialSnapshotIngestion = {
  actions: AppAction[];
  warnings: ProtocolMappingWarning[];
  requiresNativeSurface: boolean;
  newTaskProjectSelection: InitialNewTaskProjectSelection;
};

export type InitialSnapshotIngestionOptions = {
  includeActiveTask?: boolean;
  retainedNewTaskContext?: NewTaskContextIds;
};

export function actionsFromInitialSnapshot(
  snapshot: ClientSnapshot,
  options: InitialSnapshotIngestionOptions = {},
): InitialSnapshotIngestion {
  const includeActiveTask = options.includeActiveTask ?? true;
  const global = snapshot.global;
  const context = {
    agents: global.agents.agents,
    projects: global.projects.projects,
  };
  const actions: AppAction[] = [];
  const warnings: ProtocolMappingWarning[] = [];
  let requiresNativeSurface = false;
  const projects = global.projects.projects.map((project) => ({
    projectId: project.projectId,
    label: project.label,
    workspaceRoot: project.workspaceRoot,
    available: project.available,
    worktreeRepositoryId: project.worktreeRepositoryId ?? undefined,
    projectWorktreeId: project.projectWorktreeId ?? undefined,
    worktreeError: project.worktreeError ?? undefined,
  }));
  const shellProjectId = snapshot.client.surface.kind === "newTask"
    ? snapshot.client.surface.projectId ?? undefined
    : undefined;
  const initialContext = selectInitialNewTaskContext({
    retained: options.retainedNewTaskContext,
    shellProjectId,
    defaults: global.newTaskDefaults,
    projects,
    agents: global.agents.agents,
  });
  const newTaskProjectSelection = selectInitialNewTaskProject({
    retained: options.retainedNewTaskContext,
    shellProjectId,
    defaults: global.newTaskDefaults,
    projects,
  });

  actions.push({
    type: "projects",
    initialProjectId: initialContext.projectId,
    projects,
  });

  if (initialContext.agentId) {
    const agent = global.agents.agents.find((candidate) => candidate.agentId === initialContext.agentId);
    actions.push({
      type: "newTask:agent",
      agentId: initialContext.agentId,
      agentLabel: agent?.label ?? initialContext.agentId,
    });
  }

  if (global.settings.runtime) {
    actions.push({
      type: "settings:runtimeSettings",
      settings: mapProtocolRuntimeSettings(global.settings.runtime),
    });
  }

  if (global.settings.sections.length) {
    actions.push({
      type: "settings:sections",
      tabs: mapSettingsSections(global.settings.sections),
    });
  }

  if (global.settings.preferences) {
    actions.push({
      type: "settings:preferences",
      preferences: mapProtocolAppPreferences(global.settings.preferences),
    });
  }

  // The Frontend renders these areas live, so a baseline installs them as a change would.
  actions.push(agentCollectionAction(global.agents));
  actions.push({ type: "settings:devices", devices: global.devices });
  actions.push({ type: "terminals", terminals: global.terminals });
  for (const repository of global.worktrees) actions.push({ type: "worktreeRepository", repository });

  if (includeActiveTask && snapshot.activeTask) {
    const mapped = mapProtocolTaskSnapshot(snapshot.activeTask, context);
    actions.push({ type: "snapshot", snapshot: mapped.snapshot, intent: "open" });
    warnings.push(...mapped.warnings);
    requiresNativeSurface ||= mapped.requiresNativeSurface;
  }

  return { actions, warnings, requiresNativeSurface, newTaskProjectSelection };
}
