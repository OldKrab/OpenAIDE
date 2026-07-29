import type {
  WorktreeRemovalPreflight,
  WorktreeRepositorySnapshot,
  WorktreeSummary,
} from "@openaide/app-server-client";
import type { TaskSummary } from "@openaide/app-shell-contracts";
import {
  AlertTriangle,
  ArrowLeft,
  Copy,
  ExternalLink,
  FolderGit2,
  FolderOpen,
  GitBranch,
  LockKeyhole,
  Plus,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import type { ProjectOption } from "../../state/composerOptions";
import { relativeTime } from "../taskSurfaceHelpers";
import type { WorktreeSettingsIntents } from "./WorktreesSettingsTab";
import { InlineFailure } from "./settingsPresentation";

export function WorktreeSettingsDetail({
  intents,
  onBack,
  onNewTask,
  onRecreate,
  project,
  repository,
  worktree,
}: {
  intents: WorktreeSettingsIntents;
  onBack: () => void;
  onNewTask: () => void;
  onRecreate: () => void;
  project: ProjectOption;
  repository: WorktreeRepositorySnapshot;
  worktree: WorktreeSummary;
}) {
  const [tasks, setTasks] = useState<TaskSummary[]>([]);
  const [tasksError, setTasksError] = useState<string>();
  const [preflight, setPreflight] = useState<WorktreeRemovalPreflight>();
  const [error, setError] = useState<string>();
  const linked = useMemo(
    () => tasks.filter((task) => task.worktree_id === worktree.worktreeId),
    [tasks, worktree.worktreeId],
  );
  useEffect(() => {
    if (!intents.loadProjectTasks) return;
    let active = true;
    setTasksError(undefined);
    void intents.loadProjectTasks(project.projectId)
      .then((result) => {
        if (active) setTasks(result);
      })
      .catch((cause) => {
        if (active) setTasksError(cause instanceof Error ? cause.message : "Unable to load linked tasks.");
      });
    return () => {
      active = false;
    };
  }, [intents, project.projectId]);

  return (
    <article className="settings-worktree-detail">
      <button
        aria-label="Back to Worktrees"
        className="settings-detail-back"
        onClick={onBack}
        type="button"
      >
        <ArrowLeft size={14} /> Back to Worktrees
      </button>
      <header className="settings-worktree-detail-header">
        <span className="settings-worktree-detail-icon"><FolderGit2 size={19} /></span>
        <span>
          <h2>{worktree.name}</h2>
          <small><GitBranch size={11} /> {headLabel(worktree)}</small>
        </span>
        {worktree.availability === "available" ? (
          <button className="worktree-primary" onClick={onNewTask} type="button">
            <Plus size={14} /> New task here
          </button>
        ) : null}
      </header>
      {worktree.availability === "unavailable" || worktree.lockedReason ? (
        <section className="settings-worktree-notice">
          {worktree.availability === "unavailable"
            ? <AlertTriangle size={15} />
            : <LockKeyhole size={15} />}
          <span>
            <strong>{worktree.availability === "unavailable" ? "Folder cannot be found" : "Worktree is locked"}</strong>
            <small>{worktree.availabilityReason ?? worktree.lockedReason}</small>
          </span>
          <button onClick={() => void intents.refreshWorktrees(project)} type="button">
            <RefreshCw size={13} /> Check again
          </button>
          {worktree.availability === "unavailable" ? (
            <button onClick={onRecreate} type="button">Recreate</button>
          ) : null}
        </section>
      ) : null}
      {error ? <InlineFailure message={error} /> : null}
      <section className="settings-worktree-facts">
        <div className="settings-worktree-path">
          <header>
            <span><FolderOpen size={13} /> Worktree folder</span>
            <span>
              <button
                aria-label="Copy path"
                onClick={() => void navigator.clipboard.writeText(worktree.path)}
                title="Copy path"
                type="button"
              >
                <Copy size={13} />
              </button>
              {intents.openFolder ? (
                <button
                  aria-label="Open folder"
                  disabled={worktree.availability === "unavailable"}
                  onClick={() => intents.openFolder?.(repository.repositoryId, worktree.worktreeId)}
                  title="Open folder"
                  type="button"
                >
                  <FolderOpen size={13} />
                </button>
              ) : null}
            </span>
          </header>
          <code>{worktree.path}</code>
        </div>
        <dl>
          <div><dt>Branch</dt><dd>{headLabel(worktree)}</dd></div>
          <div><dt>Type</dt><dd>{capitalize(worktree.ownership)} worktree</dd></div>
          <div><dt>Last activity</dt><dd>{worktree.lastUsedAt ? relativeTime(worktree.lastUsedAt) : "Never"}</dd></div>
        </dl>
      </section>
      <section className="settings-worktree-tasks">
        <header><h3>Linked tasks</h3><span>{linked.length}</span></header>
        <div>
          {linked.map((task) => (
            <button key={task.task_id} onClick={() => intents.openTask(task.task_id)} type="button">
              <span><strong>{task.title}</strong><small>{task.agent_name}</small></span>
              <small>{task.status === "active" ? "Running" : relativeTime(task.last_activity)}</small>
              <ExternalLink size={13} />
            </button>
          ))}
          {!linked.length && !tasksError ? <p>No linked tasks.</p> : null}
          {tasksError ? <p className="worktree-error">{tasksError}</p> : null}
        </div>
      </section>
      <footer className="settings-worktree-detail-footer">
        {!preflight ? (
          <button
            className="worktree-danger"
            onClick={async () => {
              setError(undefined);
              try {
                setPreflight(await intents.removalPreflight(
                  repository.repositoryId,
                  worktree.worktreeId,
                ));
              } catch (cause) {
                setError(cause instanceof Error ? cause.message : "Unable to inspect worktree.");
              }
            }}
            type="button"
          >
            <Trash2 size={13} /> {worktree.availability === "unavailable" ? "Forget worktree" : "Remove worktree"}
          </button>
        ) : (
          <RemovalConfirmation
            onCancel={() => setPreflight(undefined)}
            onConfirm={async () => {
              try {
                await intents.removeWorktree(repository.repositoryId, worktree.worktreeId);
                onBack();
              } catch (cause) {
                setError(cause instanceof Error ? cause.message : "Unable to remove worktree.");
              }
            }}
            preflight={preflight}
            worktree={worktree}
          />
        )}
      </footer>
    </article>
  );
}

function RemovalConfirmation({
  onCancel,
  onConfirm,
  preflight,
  worktree,
}: {
  onCancel: () => void;
  onConfirm: () => Promise<void>;
  preflight: WorktreeRemovalPreflight;
  worktree: WorktreeSummary;
}) {
  const forgetting = worktree.availability === "unavailable";
  return (
    <section className="settings-worktree-remove-confirm">
      <span>
        <strong>{preflight.status === "safe"
          ? `${forgetting ? "Forget" : "Remove"} “${worktree.name}”?`
          : "Worktree cannot be removed"}</strong>
        <small>{preflight.status === "safe"
          ? forgetting
            ? "The missing folder remains untouched. Linked task history is kept."
            : "The folder will be deleted. The branch and linked task history are kept."
          : blockerText(preflight)}</small>
      </span>
      <button onClick={onCancel} type="button">Cancel</button>
      {preflight.status === "safe" ? (
        <button className="worktree-danger filled" onClick={() => void onConfirm()} type="button">
          {forgetting ? "Forget" : "Remove"}
        </button>
      ) : null}
    </section>
  );
}

function headLabel(worktree: WorktreeSummary) {
  return worktree.head.kind === "branch"
    ? `${worktree.head.name} · ${worktree.head.commit.slice(0, 7)}`
    : `Detached · ${worktree.head.commit.slice(0, 7)}`;
}

function blockerText(preflight: WorktreeRemovalPreflight) {
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

function capitalize(value: string) {
  return `${value.slice(0, 1).toUpperCase()}${value.slice(1)}`;
}
