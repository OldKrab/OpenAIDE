export { projectIdForWorkspaceRoot } from "@openaide/app-shell-contracts";
import { projectIdForWorkspaceRoot } from "@openaide/app-shell-contracts";

export function workspaceLabel(workspaceRoot: string) {
  const trimmedSeparators = workspaceRoot.replace(/[\\/]+$/u, "");
  const segments = trimmedSeparators.split(/[\\/]+/u);
  return segments.at(-1)?.trim() || "Project";
}

export function workspaceRootForProjectId(projectId: string | undefined, workspaceRoot: string) {
  const trimmedRoot = workspaceRoot.trim();
  if (!projectId || !trimmedRoot) return undefined;
  return projectIdForWorkspaceRoot(trimmedRoot) === projectId ? trimmedRoot : undefined;
}
