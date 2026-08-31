import type { WebviewSurface } from "./surfaceTypes";

type SurfaceState = {
  surface: WebviewSurface;
  taskId?: string;
  shell?: { kind: "desktop" | "web" | "vscodeExtension" };
};

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
  return shouldLoadTaskNavigation(bootstrap)
    && projectId !== undefined
    && projectId.trim().length > 0;
}

/** Project shells render Task Navigation; native VS Code editor panels do not. */
export function shouldLoadTaskNavigation(bootstrap: SurfaceState) {
  if (bootstrap.surface === "invalid") return false;
  return bootstrap.surface === "navigation" || bootstrap.shell?.kind === "web" || bootstrap.shell?.kind === "desktop";
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
