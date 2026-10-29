import { useState } from "react";
import { Check, ChevronDown, Folder, FolderGit2, GitBranch } from "lucide-react";
import type { FileWorkspace } from "./projectFilesModel";
import { useBackNavigation } from "./useBackNavigation";
import "../styles/app/project-files-workspace-switcher.css";

/** Chooses which workspace Project files shows: the Project root or one of its worktrees. A
 * small chip, because the view switch beside it is used far more often; on a phone the choices
 * open as a bottom sheet. */
export function ProjectFilesWorkspaceSwitcher({
  homeLabel,
  onSelect,
  suggestedId,
  workspace,
  workspaces,
}: {
  /** Marks the workspace the entry point belongs to, such as "this Task". */
  homeLabel?: string;
  onSelect: (id: string) => void;
  /** The workspace the Agent is working in, when it is not the selected one. */
  suggestedId?: string;
  workspace: FileWorkspace;
  workspaces: FileWorkspace[];
}) {
  const [open, setOpen] = useState(false);
  useBackNavigation(open, () => setOpen(false), 130);
  const icon = (item: FileWorkspace, size: number) =>
    item.kind === "Worktree" ? <FolderGit2 aria-hidden="true" size={size} /> : <Folder aria-hidden="true" size={size} />;
  const single = workspaces.length < 2;
  return (
    <div
      className="project-files-switcher"
      onKeyDown={(event) => {
        if (event.key !== "Escape" || !open) return;
        event.preventDefault();
        event.stopPropagation();
        setOpen(false);
      }}
    >
      <button
        type="button"
        className="project-files-workspace"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`Workspace: ${workspace.label}${workspace.branch ? `, ${workspace.branch}` : ""}`}
        disabled={single}
        onClick={() => setOpen(!open)}
      >
        {icon(workspace, 14)}
        <strong>{workspace.label}</strong>
        {workspace.branch && (
          <span>
            <GitBranch aria-hidden="true" size={12} />
            {workspace.branch}
          </span>
        )}
        {!single && <ChevronDown aria-hidden="true" size={13} />}
      </button>
      {open && (
        <>
          <div className="project-files-sheet-backdrop" onClick={() => setOpen(false)} />
          <div className="project-files-menu" role="listbox" aria-label="Show files from">
            <p className="project-files-sheet-title">Show files from</p>
            {workspaces.map((item) => (
              <button
                type="button"
                key={item.id}
                role="option"
                aria-selected={item.id === workspace.id}
                onClick={() => {
                  setOpen(false);
                  if (item.id !== workspace.id) onSelect(item.id);
                }}
              >
                {icon(item, 14)}
                <span>
                  <strong>{item.label}</strong>
                  <small>
                    {[
                      item.kind,
                      item.branch,
                      item.taskWorkspace && homeLabel,
                      item.id === suggestedId && "agent is working here",
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </small>
                </span>
                {item.id === workspace.id && <Check aria-hidden="true" size={14} />}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
