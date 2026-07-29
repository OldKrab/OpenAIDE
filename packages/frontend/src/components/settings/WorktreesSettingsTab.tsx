import type {
  WorktreeRepositorySnapshot,
  WorktreeSummary,
} from "@openaide/app-server-client";
import {
  AlertTriangle,
  ChevronRight,
  Copy,
  FolderGit2,
  FolderOpen,
  LockKeyhole,
  Plus,
  RefreshCw,
} from "lucide-react";
import { useState } from "react";

import type { NewTaskViewIntents } from "../NewTaskView";
import { copyText } from "../clipboard";
import type { ProjectOption } from "../../state/composerOptions";
import { InlineFailure } from "./settingsPresentation";
import { WorktreeCreateDialog } from "./WorktreeCreateDialog";
import { WorktreeSettingsDetail } from "./WorktreeSettingsDetail";

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

type Selection = { projectId: string; worktreeId: string };

export function WorktreesSettingsTab({
  intents,
  onNewTask,
  projects,
  repositories,
}: {
  intents: WorktreeSettingsIntents;
  onNewTask: (project: ProjectOption, worktree: WorktreeSummary) => void;
  projects: ProjectOption[];
  repositories: Record<string, WorktreeRepositorySnapshot>;
}) {
  const supported = projects.filter((project) => project.worktreeRepositoryId);
  const [selection, setSelection] = useState<Selection>();
  const [createTarget, setCreateTarget] = useState<{
    project: ProjectOption;
    recreate?: WorktreeSummary;
  }>();
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string>();
  const selectedProject = projects.find((project) => project.projectId === selection?.projectId);
  const selectedRepository = selectedProject?.worktreeRepositoryId
    ? repositories[selectedProject.worktreeRepositoryId]
    : undefined;
  const selectedWorktree = selectedRepository?.worktrees.find(
    (worktree) => worktree.worktreeId === selection?.worktreeId,
  );

  if (selectedProject && selectedRepository && selectedWorktree) {
    return (
      <WorktreeSettingsDetail
        intents={intents}
        onBack={() => setSelection(undefined)}
        onNewTask={() => onNewTask(selectedProject, selectedWorktree)}
        onRecreate={() => setCreateTarget({ project: selectedProject, recreate: selectedWorktree })}
        project={selectedProject}
        repository={selectedRepository}
        worktree={selectedWorktree}
      />
    );
  }

  const refreshAll = async () => {
    setRefreshing(true);
    setError(undefined);
    try {
      const results = await Promise.allSettled(supported.map((project) => intents.refreshWorktrees(project)));
      const failed = results.find((result) => result.status === "rejected");
      if (failed?.status === "rejected") throw failed.reason;
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to refresh worktrees.");
    } finally {
      setRefreshing(false);
    }
  };

  return (
    <section className="settings-worktrees">
      <div className="settings-worktrees-tools">
        <button
          aria-label="Refresh worktrees"
          className={refreshing ? "refreshing" : ""}
          disabled={refreshing}
          onClick={() => void refreshAll()}
          title="Refresh worktrees"
          type="button"
        >
          <RefreshCw size={14} />
        </button>
        <button
          className="worktree-primary"
          disabled={!supported.length}
          onClick={() => setCreateTarget({ project: supported[0] })}
          type="button"
        >
          <Plus size={14} /> New worktree
        </button>
      </div>
      {error ? <InlineFailure message={error} /> : null}
      <div className="settings-worktree-projects">
        {supported.map((project) => {
          const repository = repositories[project.worktreeRepositoryId!];
          const worktrees = visibleWorktrees(project, repository);
          if (!worktrees.length) return null;
          return (
            <section className="settings-worktree-project" key={project.projectId}>
              <header>
                <FolderGit2 size={14} />
                <strong>{repositoryLabel(project, repository)}</strong>
                <span>{worktrees.length}</span>
              </header>
              <div>
                {worktrees.map((worktree) => (
                  <div
                    className="settings-worktree-row"
                    key={worktree.worktreeId}
                  >
                    <span className="settings-worktree-copy">
                      <strong>{worktree.name}</strong>
                      <small className="settings-worktree-location">
                        <FolderOpen size={11} />
                        <span>{worktreeLocation(worktree)}</span>
                      </small>
                      <button
                        aria-label={`Copy full path for ${worktree.name}`}
                        className="settings-worktree-copy-path"
                        onClick={async () => {
                          setError(undefined);
                          try {
                            await copyText(worktree.path);
                          } catch (cause) {
                            setError(cause instanceof Error ? cause.message : "Unable to copy worktree path.");
                          }
                        }}
                        title="Copy full path"
                        type="button"
                      >
                        <Copy size={12} />
                      </button>
                    </span>
                    {worktree.linkedTaskCount || worktree.runningTaskCount ? (
                      <span className={`settings-worktree-activity${worktree.runningTaskCount ? " running" : ""}`}>
                        <strong>{worktree.linkedTaskCount} {worktree.linkedTaskCount === 1 ? "task" : "tasks"}</strong>
                        {worktree.runningTaskCount ? <small>{worktree.runningTaskCount} running</small> : null}
                      </span>
                    ) : null}
                    {worktree.availability === "unavailable" ? (
                      <span className="worktree-exception unavailable">
                        <AlertTriangle size={11} /> Unavailable
                      </span>
                    ) : worktree.lockedReason ? (
                      <span className="worktree-exception locked">
                        <LockKeyhole size={11} /> Locked
                      </span>
                    ) : null}
                    <ChevronRight size={14} />
                    <button
                      aria-label={`Open ${worktree.name}`}
                      className="settings-worktree-open"
                      onClick={() => setSelection({
                        projectId: project.projectId,
                        worktreeId: worktree.worktreeId,
                      })}
                      type="button"
                    />
                  </div>
                ))}
              </div>
            </section>
          );
        })}
        {!supported.some((project) => visibleWorktrees(
          project,
          repositories[project.worktreeRepositoryId!],
        ).length) ? (
          <div className="settings-worktrees-empty">
            <FolderGit2 size={18} />
            <strong>No worktrees</strong>
            <span>Create a worktree to isolate parallel work.</span>
          </div>
        ) : null}
      </div>
      {createTarget ? (
        <WorktreeCreateDialog
          initialProject={createTarget.project}
          intents={intents}
          onClose={() => setCreateTarget(undefined)}
          projects={supported}
          recreate={createTarget.recreate}
          repositories={repositories}
        />
      ) : null}
    </section>
  );
}

function visibleWorktrees(project: ProjectOption, repository?: WorktreeRepositorySnapshot) {
  return (repository?.worktrees ?? []).filter((worktree) => (
    !worktree.forgotten && !worktree.isMain && !isProjectRoot(project, worktree)
  ));
}

function isProjectRoot(project: ProjectOption, worktree: WorktreeSummary) {
  return project.projectWorktreeId
    ? worktree.worktreeId === project.projectWorktreeId
    : worktree.path === project.workspaceRoot;
}

function repositoryLabel(
  project: ProjectOption,
  repository?: WorktreeRepositorySnapshot,
) {
  const mainPath = repository?.worktrees.find((worktree) => worktree.isMain)?.path;
  if (!mainPath) return project.label;
  const segments = mainPath.replace(/[\\/]+$/, "").split(/[\\/]/);
  return segments.at(-1) || project.label;
}

function worktreeLocation(worktree: WorktreeSummary) {
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
