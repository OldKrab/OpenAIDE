import type {
  WorktreeRemovalPreflight,
  WorktreeRepositorySnapshot,
  WorktreeSummary,
} from "@openaide/app-server-client";
import type { TaskSummary } from "@openaide/app-shell-contracts";
import { Copy, FolderOpen, GitBranch, Plus } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { ProjectOption } from "../../state/composerOptions";
import { copyText } from "../clipboard";
import { PopupDialog } from "../Popup";
import { relativeTime } from "../taskSurfaceHelpers";
import {
  errorSentence,
  SettingsButton,
  SettingsConfirm,
  SettingsCrumb,
  SettingsHealth,
  SettingsIdentity,
  SettingsInlineError,
  SettingsRowView,
} from "./settingsKit";
import { blockerText, headLabel, type WorktreeSettingsIntents } from "./worktreeSettingsModel";

/** One worktree: where it is, what works in it, and the checked way to remove it. */
export function WorktreePage({
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
  const [checking, setChecking] = useState(false);
  const [copied, setCopied] = useState(false);
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
        if (active) setTasksError(errorSentence(cause, "Unable to load linked tasks."));
      });
    return () => {
      active = false;
    };
  }, [intents, project.projectId]);

  const unavailable = worktree.availability === "unavailable";
  const forgetting = unavailable;
  const inspect = async () => {
    setError(undefined);
    setChecking(true);
    try {
      // Git decides whether removal is safe; the question is asked only after it answers.
      setPreflight(await intents.removalPreflight(repository.repositoryId, worktree.worktreeId));
    } catch (cause) {
      setError(errorSentence(cause, "Unable to inspect worktree."));
    } finally {
      setChecking(false);
    }
  };
  const value = (text: string) => <span className="settings-value">{text}</span>;

  return (
    <article aria-label="Worktree details" className="settings-detail">
      <SettingsCrumb label="Worktrees" onBack={onBack} />
      <SettingsIdentity
        end={worktree.availability === "available" ? (
          <SettingsButton icon={<Plus aria-hidden="true" size={14} />} onClick={onNewTask} variant="primary">New task here</SettingsButton>
        ) : undefined}
        icon={<GitBranch aria-hidden="true" size={24} strokeWidth={1.5} />}
        subtitle={project.label}
        title={worktree.name}
      />
      {unavailable || worktree.lockedReason ? (
        <SettingsHealth
          status={unavailable ? "Folder cannot be found" : "Worktree is locked"}
          text={worktree.availabilityReason ?? worktree.lockedReason}
          tone="warn"
        >
          <SettingsButton onClick={() => void intents.refreshWorktrees(project)}>Check again</SettingsButton>
          {unavailable ? <SettingsButton onClick={onRecreate}>Recreate</SettingsButton> : null}
        </SettingsHealth>
      ) : null}
      <section className="settings-group">
        <ul className="settings-card">
          <SettingsRowView
            row={{
              id: "folder",
              label: "Folder",
              control: (
                <>
                  <code className="settings-mono" title={worktree.path}>{worktree.path}</code>
                  <SettingsButton
                    ariaLabel="Copy path"
                    icon={<Copy aria-hidden="true" size={13} />}
                    onClick={() => { void copyText(worktree.path).then(() => setCopied(true), () => setError("Unable to copy the path.")); }}
                    variant="quiet"
                  >
                    {copied ? "Copied" : "Copy"}
                  </SettingsButton>
                  {intents.openFolder ? (
                    <SettingsButton
                      ariaLabel="Open folder"
                      disabled={unavailable}
                      icon={<FolderOpen aria-hidden="true" size={13} />}
                      onClick={() => intents.openFolder?.(repository.repositoryId, worktree.worktreeId)}
                    >
                      Open
                    </SettingsButton>
                  ) : null}
                </>
              ),
            }}
          />
          <SettingsRowView row={{ id: "branch", label: "Branch", control: <code className="settings-mono">{headLabel(worktree)}</code> }} />
          <SettingsRowView row={{ id: "created-by", label: "Created by", control: value(worktree.ownership === "managed" ? "OpenAIDE" : "Outside OpenAIDE") }} />
          <SettingsRowView row={{ id: "last-activity", label: "Last activity", control: value(worktree.lastUsedAt ? relativeTime(worktree.lastUsedAt) : "Never") }} />
        </ul>
      </section>
      <section className="settings-group">
        <header><h2>Tasks here</h2></header>
        {tasksError ? <SettingsInlineError>{tasksError}</SettingsInlineError> : null}
        {linked.length ? (
          <ul aria-label="Tasks here" className="settings-card">
            {linked.map((task) => (
              <SettingsRowView
                key={task.task_id}
                row={{
                  id: task.task_id,
                  label: task.title,
                  description: task.agent_name,
                  status: task.status === "active"
                    ? { tone: "ok", text: "Running" }
                    : task.status === "background"
                      ? { tone: "ok", text: "Background" }
                      : { tone: "off", text: relativeTime(task.last_activity) },
                  onOpen: () => intents.openTask(task.task_id),
                }}
              />
            ))}
          </ul>
        ) : tasksError ? null : <p className="settings-empty">None.</p>}
      </section>
      {error ? <SettingsInlineError>{error}</SettingsInlineError> : null}
      <footer className="settings-detail-footer">
        <SettingsButton busy={checking} danger onClick={() => void inspect()} variant="quiet">
          {forgetting ? "Forget worktree…" : "Remove worktree…"}
        </SettingsButton>
      </footer>
      {preflight?.status === "safe" ? (
        <SettingsConfirm
          confirmLabel={forgetting ? "Forget" : "Remove"}
          detail={forgetting
            ? "The missing folder remains untouched. Linked task history is kept."
            : "The folder will be deleted. The branch and linked task history are kept."}
          label={`${forgetting ? "Forget" : "Remove"} worktree confirmation`}
          onClose={() => setPreflight(undefined)}
          onConfirm={async () => {
            try {
              await intents.removeWorktree(repository.repositoryId, worktree.worktreeId);
            } catch (cause) {
              throw new Error(errorSentence(cause, "Unable to remove worktree."));
            }
            onBack();
          }}
          pendingLabel={forgetting ? "Forgetting…" : "Removing…"}
          title={`${forgetting ? "Forget" : "Remove"} “${worktree.name}”?`}
        />
      ) : preflight ? (
        <PopupDialog className="settings-confirm" label="Worktree cannot be removed" onOpenChange={(open) => { if (!open) setPreflight(undefined); }} open>
          <strong>Worktree cannot be removed</strong>
          <p>{blockerText(preflight)}</p>
          <footer><SettingsButton onClick={() => setPreflight(undefined)}>Close</SettingsButton></footer>
        </PopupDialog>
      ) : null}
    </article>
  );
}
