import type { WorktreeRemovalPreflight, WorktreeRepositorySnapshot, WorktreeSummary } from "@openaide/app-server-client";
import type { NewTaskViewIntents } from "../NewTaskView";
import type { ProjectOption } from "../../state/composerOptions";

export type WorktreeSettingsIntents = Pick<
  NewTaskViewIntents,
  | "createWorktree"
  | "loadProjectTasks"
  | "openFolder"
  | "openTask"
  | "recreateWorktree"
  | "refreshWorktrees"
  | "removalPreflight"
  | "removeWorktree"
>;

/** The worktrees a user made on purpose: not the Project's own checkout, not forgotten ones. */
export function visibleWorktrees(project: ProjectOption, repository?: WorktreeRepositorySnapshot) {
  return (repository?.worktrees ?? []).filter((worktree) => (
    !worktree.forgotten && !worktree.isMain && !isProjectRoot(project, worktree)
  ));
}

function isProjectRoot(project: ProjectOption, worktree: WorktreeSummary) {
  return project.projectWorktreeId
    ? worktree.worktreeId === project.projectWorktreeId
    : worktree.path === project.workspaceRoot;
}

export function repositoryLabel(project: ProjectOption, repository?: WorktreeRepositorySnapshot) {
  const mainPath = repository?.worktrees.find((worktree) => worktree.isMain)?.path;
  if (!mainPath) return project.label;
  const segments = mainPath.replace(/[\\/]+$/, "").split(/[\\/]/);
  return segments.at(-1) || project.label;
}

export function worktreeLocation(worktree: WorktreeSummary) {
  if (worktree.ownership === "managed") return "Managed by OpenAIDE";
  if (isOpenAideStoragePath(worktree.path)) return "OpenAIDE storage";
  return compactExternalPath(worktree.path);
}

function isOpenAideStoragePath(path: string) {
  // Another App Server can discover an OpenAIDE-created worktree as externally owned.
  return /(?:^|[\\/])state[\\/]worktrees[\\/]repository-[^\\/]+[\\/]worktree-[^\\/]+[\\/]?$/.test(path);
}

function compactExternalPath(path: string) {
  const abbreviated = path
    .replace(/^\/(?:home|Users)\/[^/]+(?=\/|$)/, "~")
    .replace(/^[A-Za-z]:\\Users\\[^\\]+(?=\\|$)/, "~");
  if (abbreviated.length <= 44) return abbreviated;

  const separator = abbreviated.includes("\\") && !abbreviated.includes("/") ? "\\" : "/";
  const segments = abbreviated.split(/[\\/]+/).filter(Boolean);
  if (segments.length <= 4) return abbreviated;

  // Keep enough context to recognize both the source area and the worktree itself.
  const prefix = abbreviated.startsWith("~")
    ? segments.slice(0, 2).join(separator)
    : abbreviated.startsWith(separator)
      ? `${separator}${segments[0]}`
      : segments[0];
  return `${prefix}${separator}…${separator}${segments.slice(-2).join(separator)}`;
}

export function headLabel(worktree: WorktreeSummary) {
  return worktree.head.kind === "branch"
    ? `${worktree.head.name} · ${worktree.head.commit.slice(0, 7)}`
    : `Detached · ${worktree.head.commit.slice(0, 7)}`;
}

export function blockerText(preflight: WorktreeRemovalPreflight) {
  const labels: Record<WorktreeRemovalPreflight["blockers"][number], string> = {
    detachedCommits: "Detached commits are not preserved by a branch.",
    initializedSubmodules: "Initialized submodules must be removed first.",
    locked: "Git has locked this worktree.",
    primaryWorktree: "The Project root cannot be removed.",
    runningTasks: "A linked task is running.",
    unavailable: "The folder is unavailable.",
    workingTreeChanges: "The worktree has uncommitted changes.",
  };
  return preflight.blockers.map((blocker) => labels[blocker]).join(" ");
}
