import type { WebviewSurface } from "./surfaceTypes";

type SurfaceState = { surface: WebviewSurface; taskId?: string };

export function shouldLoadNewTaskConfigOptions(
  bootstrap: SurfaceState,
  hasSnapshot: boolean,
  projectId: string | undefined,
) {
  return (
    bootstrap.surface === "task" &&
    !bootstrap.taskId &&
    !hasSnapshot &&
    projectId !== undefined &&
    projectId.trim().length > 0
  );
}

export function shouldLoadNativeSessions(
  bootstrap: SurfaceState,
  projectId: string | undefined,
) {
  return bootstrap.surface !== "invalid" && projectId !== undefined && projectId.trim().length > 0;
}

export function shouldRequestWorkspaceRoots(bootstrap: SurfaceState) {
  return bootstrap.surface !== "invalid";
}

export function configOptionsRequestKey(agentId: string, projectId: string, workspaceRoot?: string) {
  return workspaceRoot
    ? `${agentId}\u0000${projectId}\u0000${workspaceRoot}`
    : `${agentId}\u0000${projectId}`;
}

export function agentProjectRequestKey(agentId: string, projectId: string) {
  return `${agentId}\u0000${projectId}`;
}
