import type { NavigationViewParams, ProjectId, TaskId } from "@openaide/app-server-client";
import type { WebviewBootstrap } from "./surfaceTypes";
import type { AppState } from "./store";
import { initialTaskNavigationRowsPerProject } from "./taskNavigationPolicy";

/** The Projects a shell fixes for its Navigation view, or undefined to show every Project. */
export function fixedNavigationProjectIds(bootstrap: WebviewBootstrap): ProjectId[] | undefined {
  if (bootstrap.surface === "invalid" || bootstrap.shell.navigationMode !== "currentProject") return undefined;
  return bootstrap.projectIds?.map((projectId) => projectId as ProjectId)
    ?? (bootstrap.projectId ? [bootstrap.projectId as ProjectId] : undefined);
}

/**
 * The parameters App Server needs to compute this client's Navigation view: which Projects, how
 * many history rows each shows, and the open Task, which keeps its place whatever the limit.
 * Everything here is presentation intent; App Server decides the window, order and counts.
 */
export function navigationViewParams(
  state: Pick<AppState, "activeTaskId" | "navigationHistoryLimits" | "projects">,
  bootstrap: WebviewBootstrap,
  section: "tasks" | "archive",
): NavigationViewParams {
  const projectIds = fixedNavigationProjectIds(bootstrap);
  const projectCount = projectIds?.length ?? state.projects.length;
  return {
    section,
    ...(projectIds ? { projectIds } : {}),
    historyLimit: initialTaskNavigationRowsPerProject(projectCount),
    historyLimits: Object.entries(state.navigationHistoryLimits)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([projectId, historyLimit]) => ({ projectId: projectId as ProjectId, historyLimit })),
    ...(state.activeTaskId ? { openRow: { kind: "task" as const, taskId: state.activeTaskId as TaskId } } : {}),
  };
}
