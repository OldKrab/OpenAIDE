import type { ClientSnapshot } from "@openaide/app-server-client";
import type { AppAction } from "./appReducer";
import {
  mapProtocolTaskNavigation,
  mapProtocolTaskSnapshot,
  type ProtocolMappingWarning,
} from "./appServerProtocolMapping";
import { mapProtocolAppPreferences } from "./appPreferencesMapping";
import { mapProtocolRuntimeSettings } from "./runtimeSettingsMapping";
import { mapSettingsSections } from "./settingsProjectionMapping";
import {
  selectInitialNewTaskContext,
  type NewTaskContextIds,
} from "./newTaskSelectionDefaults";

export type InitialSnapshotIngestion = {
  actions: AppAction[];
  warnings: ProtocolMappingWarning[];
  requiresNativeSurface: boolean;
};

export type InitialSnapshotIngestionOptions = {
  includeTaskNavigation?: boolean;
  includeActiveTask?: boolean;
  retainedNewTaskContext?: NewTaskContextIds;
};

export function actionsFromInitialSnapshot(
  snapshot: ClientSnapshot,
  options: InitialSnapshotIngestionOptions = {},
): InitialSnapshotIngestion {
  const includeTaskNavigation = options.includeTaskNavigation ?? true;
  const includeActiveTask = options.includeActiveTask ?? true;
  const context = {
    agents: snapshot.agents?.agents,
    projects: snapshot.projects?.projects,
  };
  const actions: AppAction[] = [];
  const warnings: ProtocolMappingWarning[] = [];
  let requiresNativeSurface = false;
  const projects = snapshot.projects?.projects.map((project) => ({
    projectId: project.projectId,
    label: project.label,
    workspaceRoot: project.workspaceRoot,
    available: project.available,
    worktreeRepositoryId: project.worktreeRepositoryId ?? undefined,
    projectWorktreeId: project.projectWorktreeId ?? undefined,
    worktreeError: project.worktreeError ?? undefined,
  })) ?? [];
  const shellProjectId = snapshot.client.surface.kind === "newTask"
    ? snapshot.client.surface.projectId ?? undefined
    : undefined;
  const initialContext = selectInitialNewTaskContext({
    retained: options.retainedNewTaskContext,
    shellProjectId,
    defaults: snapshot.newTaskDefaults,
    projects,
    agents: snapshot.agents?.agents ?? [],
  });

  if (snapshot.projects) {
    actions.push({
      type: "projects",
      initialProjectId: initialContext.projectId,
      projects,
    });
  }

  if (initialContext.agentId) {
    const agent = snapshot.agents?.agents.find((candidate) => candidate.agentId === initialContext.agentId);
    actions.push({
      type: "newTask:agent",
      agentId: initialContext.agentId,
      agentLabel: agent?.label ?? initialContext.agentId,
    });
  }

  if (snapshot.settings?.runtime) {
    actions.push({
      type: "settings:runtimeSettings",
      settings: mapProtocolRuntimeSettings(snapshot.settings.runtime),
    });
  }

  if (snapshot.settings?.sections.length) {
    actions.push({
      type: "settings:sections",
      tabs: mapSettingsSections(snapshot.settings.sections),
    });
  }

  if (snapshot.settings?.preferences) {
    actions.push({
      type: "settings:preferences",
      preferences: mapProtocolAppPreferences(snapshot.settings.preferences),
    });
  }

  if (includeTaskNavigation && snapshot.tasks) {
    const mapped = mapProtocolTaskNavigation(snapshot.tasks, context);
    actions.push({
      type: "taskNavigation",
      archived: false,
      tasks: mapped.tasks,
      sessions: mapped.sessions,
      refreshing: mapped.refreshing,
    });
    if (mapped.activeTaskId) actions.push({ type: "selection:set", taskId: mapped.activeTaskId });
    warnings.push(...mapped.warnings);
    requiresNativeSurface ||= mapped.requiresNativeSurface;
  }

  if (includeActiveTask && snapshot.activeTask) {
    const mapped = mapProtocolTaskSnapshot(snapshot.activeTask, context);
    actions.push({ type: "snapshot", snapshot: mapped.snapshot, intent: "open" });
    warnings.push(...mapped.warnings);
    requiresNativeSurface ||= mapped.requiresNativeSurface;
  }

  return { actions, warnings, requiresNativeSurface };
}
