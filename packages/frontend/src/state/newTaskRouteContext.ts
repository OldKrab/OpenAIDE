import type { AppAction } from "./appReducer";
import type { AppState } from "./store";
import type { WebviewBootstrap } from "./surfaceTypes";

type ValidWebviewBootstrap = Exclude<WebviewBootstrap, { surface: "invalid" }>;

/** Resolves URL identity through the authoritative Project/worktree inventory. */
export function newTaskWorkspaceActionForRoute(
  bootstrap: WebviewBootstrap,
  state: Pick<AppState, "newTask" | "projects" | "projectsLoaded" | "worktreeRepositories">,
  inventoryReady: boolean,
): AppAction | undefined {
  if (bootstrap.surface !== "task" || bootstrap.taskId || bootstrap.shell.kind !== "web") return undefined;
  const resolved = resolveRouteWorkspace(bootstrap, state, inventoryReady && state.projectsLoaded);
  const routedProject: { projectId?: string | null } = bootstrap.worktreeId && !bootstrap.projectId
    ? state.newTask.selection.projectId ? { projectId: null } : {}
    : resolved.projectId && resolved.projectId !== state.newTask.selection.projectId
      ? { projectId: resolved.projectId }
      : {};
  const projectIdentityChanges = "projectId" in routedProject;
  if (
    state.newTask.selection.worktreeId === resolved.worktreeId
    && state.newTask.selection.workspaceLabel === resolved.label
    && state.newTask.selection.workspaceRoot === resolved.path
    && state.newTask.workspaceResolution === resolved.resolution
    && !projectIdentityChanges
  ) return undefined;
  return {
    type: "newTask:worktree",
    ...routedProject,
    worktreeId: resolved.worktreeId,
    label: resolved.label,
    path: resolved.path,
    resolution: resolved.resolution,
  };
}

/** Prevents route reconstruction from briefly acquiring Project root or an invalid worktree. */
export function newTaskRouteWorkspaceReady(
  bootstrap: WebviewBootstrap,
  state: Pick<AppState, "newTask" | "projects" | "projectsLoaded" | "worktreeRepositories">,
  inventoryReady: boolean,
) {
  if (bootstrap.surface !== "task" || bootstrap.taskId || bootstrap.shell.kind !== "web") return true;
  const resolved = resolveRouteWorkspace(bootstrap, state, inventoryReady && state.projectsLoaded);
  if (resolved.resolution) return false;
  if (bootstrap.projectId && state.newTask.selection.projectId !== bootstrap.projectId) return false;
  if (bootstrap.worktreeId && !bootstrap.projectId) return false;
  return state.newTask.selection.worktreeId === resolved.worktreeId
    && state.newTask.workspaceResolution === undefined;
}

function resolveRouteWorkspace(
  bootstrap: ValidWebviewBootstrap,
  state: Pick<AppState, "newTask" | "projects" | "projectsLoaded" | "worktreeRepositories">,
  inventoryReady: boolean,
) {
  const projectId = bootstrap.projectId ?? (
    bootstrap.worktreeId ? undefined : state.newTask.selection.projectId
  );
  if (bootstrap.projectId && !inventoryReady) {
    return unresolvedWorkspace(bootstrap.projectId, bootstrap.worktreeId, false);
  }
  if (bootstrap.worktreeId && !inventoryReady) {
    return unresolvedWorkspace(bootstrap.projectId, bootstrap.worktreeId, false);
  }
  const project = state.projects.find((candidate) => candidate.projectId === projectId);
  if (bootstrap.worktreeId && !bootstrap.projectId) {
    return unresolvedWorkspace(undefined, bootstrap.worktreeId, inventoryReady);
  }
  if (!project && bootstrap.projectId) {
    if (bootstrap.worktreeId || inventoryReady) {
      return unresolvedWorkspace(bootstrap.projectId, bootstrap.worktreeId, inventoryReady);
    }
    return unresolvedProjectRoot(bootstrap.projectId, state);
  }
  if (!project && projectId) {
    return inventoryReady
      ? unavailableWorkspace(projectId, undefined)
      : unresolvedProjectRoot(projectId, state);
  }
  if (project?.available === false) {
    return unavailableWorkspace(project.projectId, bootstrap.worktreeId);
  }
  if (!bootstrap.worktreeId) {
    return {
      projectId,
      worktreeId: undefined,
      label: project ? "Project root" : state.newTask.selection.workspaceLabel || "Project root",
      path: project?.workspaceRoot ?? state.newTask.selection.workspaceRoot,
      resolution: undefined,
    };
  }
  if (!project?.worktreeRepositoryId) {
    return unavailableWorkspace(project?.projectId, bootstrap.worktreeId);
  }
  const repository = state.worktreeRepositories[project.worktreeRepositoryId];
  if (!repository) return unresolvedWorkspace(project.projectId, bootstrap.worktreeId, false);
  const worktree = repository.worktrees.find((candidate) => (
    candidate.worktreeId === bootstrap.worktreeId && !candidate.forgotten
  ));
  if (!worktree || worktree.availability !== "available") {
    return unavailableWorkspace(project.projectId, bootstrap.worktreeId);
  }
  return {
    projectId: project.projectId,
    worktreeId: worktree.worktreeId,
    label: worktree.name,
    path: worktree.path,
    resolution: undefined,
  };
}

function unresolvedProjectRoot(
  projectId: string,
  state: Pick<AppState, "newTask">,
) {
  const retained = state.newTask.selection.projectId === projectId;
  return {
    projectId,
    worktreeId: undefined,
    label: "Project root",
    path: retained ? state.newTask.selection.workspaceRoot : "",
    resolution: undefined,
  };
}

function unresolvedWorkspace(projectId: string | undefined, worktreeId: string | undefined, inventoryReady: boolean) {
  return inventoryReady
    ? unavailableWorkspace(projectId, worktreeId)
    : {
        projectId,
        worktreeId,
        label: "Loading workspace",
        path: "",
        resolution: "loading" as const,
      };
}

function unavailableWorkspace(projectId?: string, worktreeId?: string) {
  return {
    projectId,
    worktreeId,
    label: "Workspace unavailable",
    path: "",
    resolution: "unavailable" as const,
  };
}
