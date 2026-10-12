import type {
  WorktreeBaseSelection,
  WorktreeRepositorySnapshot,
  WorktreeSummary,
} from "@openaide/app-server-client";
import { useState } from "react";
import type { ProjectOption } from "../../state/composerOptions";
import { PopupDialog } from "../Popup";
import { errorSentence, SettingsButton, SettingsField, SettingsInlineError, SettingsSelect } from "./settingsKit";
import type { WorktreeSettingsIntents } from "./worktreeSettingsModel";

/** Creates a worktree, or recreates one whose folder is gone, from a chosen starting point. */
export function WorktreeCreateDialog({
  initialProject,
  intents,
  onClose,
  projects,
  recreate,
  repositories,
}: {
  initialProject: ProjectOption;
  intents: WorktreeSettingsIntents;
  onClose: () => void;
  projects: ProjectOption[];
  recreate?: WorktreeSummary;
  repositories: Record<string, WorktreeRepositorySnapshot>;
}) {
  const [project, setProject] = useState(initialProject);
  const repository = repositories[project.worktreeRepositoryId!];
  const [name, setName] = useState(recreate?.name ?? "");
  const [branch, setBranch] = useState(recreate?.head.kind === "branch" ? recreate.head.name : "");
  const [base, setBase] = useState<WorktreeBaseSelection>({ kind: "currentHead" });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const submit = async () => {
    setBusy(true);
    setError(undefined);
    try {
      if (recreate) {
        await intents.recreateWorktree(project, recreate.worktreeId, { base, branch: branch.trim() || undefined });
      } else {
        await intents.createWorktree(project, { name: name.trim(), base, branch: branch.trim() || undefined });
      }
      onClose();
    } catch (cause) {
      setError(errorSentence(cause, "Unable to create worktree."));
      setBusy(false);
    }
  };

  return (
    <PopupDialog
      className="settings-confirm settings-form-dialog"
      label={recreate ? "Recreate worktree" : "New worktree"}
      onOpenChange={(open) => { if (!open && !busy) onClose(); }}
      open
    >
      <strong>{recreate ? `Recreate ${recreate.name}` : "New worktree"}</strong>
      {projects.length > 1 && !recreate ? (
        <div className="settings-field">
          <span>Project</span>
          <SettingsSelect
            label="Project"
            onChange={(projectId) => setProject(projects.find((candidate) => candidate.projectId === projectId)!)}
            options={projects.map((candidate) => ({ label: candidate.label, value: candidate.projectId }))}
            value={project.projectId}
          />
        </div>
      ) : null}
      {!recreate ? (
        <SettingsField label="Name">
          <input aria-label="Name" autoFocus onChange={(event) => setName(event.currentTarget.value)} value={name} />
        </SettingsField>
      ) : null}
      <SettingsField label="Branch">
        <input aria-label="Branch" onChange={(event) => setBranch(event.currentTarget.value)} placeholder="Optional" value={branch} />
      </SettingsField>
      <div className="settings-field">
        <span>Starting point</span>
        <SettingsSelect
          label="Starting point"
          onChange={(value) => setBase(value === "head"
            ? { kind: "currentHead" }
            : { kind: "localBranch", name: value.slice("branch:".length) })}
          options={[
            { label: "Current HEAD", value: "head" },
            ...(repository?.bases ?? []).flatMap((candidate) => (
              candidate.kind === "localBranch" ? [{ label: candidate.name, value: `branch:${candidate.name}` }] : []
            )),
          ]}
          value={base.kind === "currentHead" ? "head" : `branch:${base.name}`}
        />
      </div>
      {error ? <SettingsInlineError>{error}</SettingsInlineError> : null}
      <footer>
        <SettingsButton disabled={busy} onClick={onClose}>Cancel</SettingsButton>
        <SettingsButton busy={busy} disabled={!recreate && !name.trim()} onClick={() => void submit()} variant="primary">
          {busy ? "Preparing…" : recreate ? "Recreate" : "Create worktree"}
        </SettingsButton>
      </footer>
    </PopupDialog>
  );
}
