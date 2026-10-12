import type { WorktreeRepositorySnapshot, WorktreeSummary } from "@openaide/app-server-client";
import { FolderGit2, GitBranch, Plus } from "lucide-react";
import { useEffect, useState } from "react";
import type { ProjectOption } from "../../state/composerOptions";
import { settingsInfo } from "./settingsInfo";
import { errorSentence, SettingsBlankState, SettingsButton, SettingsInlineError } from "./settingsKit";
import type { SettingsGroup, SettingsRow, SettingsSection } from "./settingsModel";
import { WorktreeCreateDialog } from "./WorktreeCreateDialog";
import { WorktreePage } from "./WorktreePage";
import { repositoryLabel, visibleWorktrees, worktreeLocation, type WorktreeSettingsIntents } from "./worktreeSettingsModel";

export type WorktreesSectionProps = {
  /** True while this section is the one on screen; entering it re-reads Git. */
  active: boolean;
  intents?: WorktreeSettingsIntents;
  onNewTask?: (project: ProjectOption, worktree: WorktreeSummary) => void;
  projects: ProjectOption[];
  repositories: Record<string, WorktreeRepositorySnapshot>;
};

type Selection = { projectId: string; worktreeId: string };

/** Every Project's worktrees in one place. Git and the App Server own them; this lists and asks. */
export function useWorktreesSection({ active, intents, onNewTask, projects, repositories }: WorktreesSectionProps): SettingsSection | undefined {
  const supported = projects.filter((project) => project.worktreeRepositoryId);
  const [selection, setSelection] = useState<Selection>();
  const [createTarget, setCreateTarget] = useState<{ project: ProjectOption; recreate?: WorktreeSummary }>();
  const [error, setError] = useState<string>();
  const enabled = Boolean(intents && onNewTask);
  useEffect(() => {
    if (!active || !intents) return;
    // Worktrees also change outside OpenAIDE, so the list is read again each time it is shown.
    let current = true;
    setError(undefined);
    void Promise.allSettled(supported.map((project) => intents.refreshWorktrees(project))).then((results) => {
      const failed = results.find((result) => result.status === "rejected");
      if (current && failed?.status === "rejected") setError(errorSentence(failed.reason, "Unable to refresh worktrees."));
    });
    return () => { current = false; };
  }, [active, intents, supported.map((project) => project.projectId).join("\n")]);
  if (!enabled || !intents || !onNewTask) return undefined;

  const selectedProject = projects.find((project) => project.projectId === selection?.projectId);
  const selectedRepository = selectedProject?.worktreeRepositoryId ? repositories[selectedProject.worktreeRepositoryId] : undefined;
  const selectedWorktree = selectedRepository?.worktrees.find((worktree) => worktree.worktreeId === selection?.worktreeId);
  const closeDetail = () => {
    setSelection(undefined);
    return true;
  };

  const groups: SettingsGroup[] = supported.flatMap((project) => {
    const repository = repositories[project.worktreeRepositoryId!];
    const worktrees = visibleWorktrees(project, repository);
    if (!worktrees.length) return [];
    return [{
      id: project.projectId,
      title: repositoryLabel(project, repository),
      rows: worktrees.map((worktree): SettingsRow => ({
        id: worktree.worktreeId,
        label: worktree.name,
        description: worktreeLocation(worktree),
        keywords: `worktree ${worktree.head.kind === "branch" ? worktree.head.name : "detached"} ${worktree.ownership} ${worktree.path}`,
        leading: <GitBranch aria-hidden="true" size={18} strokeWidth={1.6} />,
        status: worktreeStatus(worktree),
        onOpen: () => setSelection({ projectId: project.projectId, worktreeId: worktree.worktreeId }),
      })),
    }];
  });
  const showDetail = Boolean(selectedProject && selectedRepository && selectedWorktree);

  return {
    id: "worktrees",
    label: "Worktrees",
    icon: FolderGit2,
    info: settingsInfo.worktrees,
    action: (
      <SettingsButton
        disabled={!supported.length}
        icon={<Plus aria-hidden="true" size={14} />}
        onClick={() => setCreateTarget({ project: supported[0] })}
        variant="primary"
      >
        New worktree
      </SettingsButton>
    ),
    notice: (
      <>
        {error ? <SettingsInlineError>{error}</SettingsInlineError> : null}
        {!groups.length ? <SettingsBlankState detail="Create one to work on a project in parallel." title="No worktrees" /> : null}
      </>
    ),
    groups,
    closeDetail: showDetail ? closeDetail : undefined,
    detail: showDetail ? (
      <WorktreePage
        intents={intents}
        onBack={closeDetail}
        onNewTask={() => onNewTask(selectedProject!, selectedWorktree!)}
        onRecreate={() => setCreateTarget({ project: selectedProject!, recreate: selectedWorktree! })}
        project={selectedProject!}
        repository={selectedRepository!}
        worktree={selectedWorktree!}
      />
    ) : undefined,
    overlay: createTarget ? (
      <WorktreeCreateDialog
        initialProject={createTarget.project}
        intents={intents}
        onClose={() => setCreateTarget(undefined)}
        projects={supported}
        recreate={createTarget.recreate}
        repositories={repositories}
      />
    ) : undefined,
  };
}

function worktreeStatus(worktree: WorktreeSummary): SettingsRow["status"] {
  if (worktree.availability === "unavailable") return { tone: "warn", text: "Unavailable" };
  if (worktree.lockedReason) return { tone: "warn", text: "Locked" };
  if (worktree.runningTaskCount) return { tone: "ok", text: `${worktree.runningTaskCount} running` };
  if (worktree.linkedTaskCount) return { tone: "off", text: `${worktree.linkedTaskCount} ${worktree.linkedTaskCount === 1 ? "task" : "tasks"}` };
  return { tone: "off", text: "No tasks" };
}
