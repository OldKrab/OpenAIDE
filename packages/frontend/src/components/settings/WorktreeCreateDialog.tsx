import type {
  WorktreeBaseSelection,
  WorktreeRepositorySnapshot,
  WorktreeSummary,
} from "@openaide/app-server-client";
import { Check, ChevronDown, X } from "lucide-react";
import { useState } from "react";

import type { ProjectOption } from "../../state/composerOptions";
import type { WorktreeSettingsIntents } from "./WorktreesSettingsTab";
import { InlineFailure } from "./settingsPresentation";

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
  const [branch, setBranch] = useState(
    recreate?.head.kind === "branch" ? recreate.head.name : "",
  );
  const [base, setBase] = useState<WorktreeBaseSelection>({ kind: "currentHead" });
  const [projectMenu, setProjectMenu] = useState(false);
  const [baseMenu, setBaseMenu] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  return (
    <div
      className="settings-worktree-dialog-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
      role="presentation"
    >
      <section
        aria-labelledby="settings-new-worktree-title"
        aria-modal="true"
        className="settings-worktree-dialog"
        onKeyDown={(event) => {
          if (event.key === "Escape" && !busy) onClose();
        }}
        role="dialog"
      >
        <header>
          <span>
            <h2 id="settings-new-worktree-title">{recreate ? "Recreate worktree" : "New worktree"}</h2>
            <small>{project.label}</small>
          </span>
          <button aria-label="Close" disabled={busy} onClick={onClose} type="button"><X size={15} /></button>
        </header>
        {projects.length > 1 && !recreate ? (
          <DialogSelect
            label="Project"
            open={projectMenu}
            onOpen={setProjectMenu}
            options={projects.map((candidate) => ({
              label: candidate.label,
              value: candidate.projectId,
            }))}
            onSelect={(projectId) => {
              setProject(projects.find((candidate) => candidate.projectId === projectId)!);
              setProjectMenu(false);
            }}
            value={project.projectId}
          />
        ) : null}
        {!recreate ? (
          <label><span>Name</span><input autoFocus onChange={(event) => setName(event.currentTarget.value)} value={name} /></label>
        ) : null}
        <label>
          <span>Branch</span>
          <input onChange={(event) => setBranch(event.currentTarget.value)} placeholder="Optional branch name" value={branch} />
        </label>
        <DialogSelect
          label="Starting point"
          open={baseMenu}
          onOpen={setBaseMenu}
          options={[
            { label: "Current HEAD", value: "head" },
            ...(repository?.bases ?? []).flatMap((candidate) => (
              candidate.kind === "localBranch"
                ? [{ label: candidate.name, value: `branch:${candidate.name}` }]
                : []
            )),
          ]}
          onSelect={(value) => {
            setBase(value === "head"
              ? { kind: "currentHead" }
              : { kind: "localBranch", name: value.slice("branch:".length) });
            setBaseMenu(false);
          }}
          value={base.kind === "currentHead" ? "head" : `branch:${base.name}`}
        />
        {error ? <InlineFailure message={error} /> : null}
        <footer>
          <button disabled={busy} onClick={onClose} type="button">Cancel</button>
          <button
            className="worktree-primary"
            disabled={busy || (!recreate && !name.trim())}
            onClick={async () => {
              setBusy(true);
              setError(undefined);
              try {
                if (recreate) {
                  await intents.recreateWorktree(
                    project,
                    recreate.worktreeId,
                    { base, branch: branch.trim() || undefined },
                  );
                } else {
                  await intents.createWorktree(
                    project,
                    { name: name.trim(), base, branch: branch.trim() || undefined },
                  );
                }
                onClose();
              } catch (cause) {
                setError(cause instanceof Error ? cause.message : "Unable to create worktree.");
              } finally {
                setBusy(false);
              }
            }}
            type="button"
          >
            {busy ? "Preparing…" : recreate ? "Recreate" : "Create worktree"}
          </button>
        </footer>
      </section>
    </div>
  );
}

function DialogSelect({
  label,
  onOpen,
  onSelect,
  open,
  options,
  value,
}: {
  label: string;
  onOpen: (open: boolean) => void;
  onSelect: (value: string) => void;
  open: boolean;
  options: Array<{ label: string; value: string }>;
  value: string;
}) {
  const selected = options.find((option) => option.value === value)?.label ?? value;
  return (
    <label className="settings-worktree-dialog-select">
      <span>{label}</span>
      <button aria-expanded={open} onClick={() => onOpen(!open)} type="button">
        <span>{selected}</span><ChevronDown size={14} />
      </button>
      {open ? (
        <div role="listbox">
          {options.map((option) => (
            <button
              aria-selected={option.value === value}
              key={option.value}
              onClick={() => onSelect(option.value)}
              role="option"
              type="button"
            >
              <span>{option.label}</span>{option.value === value ? <Check size={13} /> : null}
            </button>
          ))}
        </div>
      ) : null}
    </label>
  );
}
