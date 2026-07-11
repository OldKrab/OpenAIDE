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

export type InitialSnapshotIngestion = {
  actions: AppAction[];
  warnings: ProtocolMappingWarning[];
  requiresNativeSurface: boolean;
};

export type InitialSnapshotIngestionOptions = {
  includeTaskNavigation?: boolean;
  includeActiveTask?: boolean;
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

  if (snapshot.projects) {
    const requestedProjectId = snapshot.client.surface.kind === "newTask"
      ? snapshot.client.surface.projectId ?? undefined
      : undefined;
    actions.push({
      type: "projects",
      activeProjectId: requestedProjectId ?? snapshot.projects.activeProjectId ?? undefined,
      projects: snapshot.projects.projects.map((project) => ({
        projectId: project.projectId,
        label: project.label,
      })),
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
    actions.push({ type: "tasks", tasks: mapped.tasks });
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
