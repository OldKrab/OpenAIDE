import { workspaceRootForProjectId } from "../state/projectIdentity";
import type { AppState } from "../state/store";

export function newTaskProjectIdForRequests(
  state: AppState,
  routeProjectId: string | undefined,
) {
  const selectedProjectId = state.newTask.selection.projectId;
  if (selectedProjectId && selectedProjectId === routeProjectId) return selectedProjectId;
  if (!selectedProjectId) return undefined;
  if (state.projects.some((project) => project.projectId === selectedProjectId)) return selectedProjectId;
  if (state.tasks.some((task) => task.project_id === selectedProjectId)) return selectedProjectId;
  return undefined;
}

/** Adds workspace context only when its deterministic identity matches an unpersisted Project. */
export function newTaskConfigOptionsContextForRequests(
  state: AppState,
  routeProjectId: string | undefined,
) {
  const projectId = newTaskProjectIdForRequests(state, routeProjectId);
  if (projectId) return { projectId };

  const selectedProjectId = state.newTask.selection.projectId;
  const workspaceRoot = workspaceRootForProjectId(
    selectedProjectId,
    state.newTask.selection.workspaceRoot,
  );
  return selectedProjectId && workspaceRoot
    ? { projectId: selectedProjectId, workspaceRoot }
    : undefined;
}
