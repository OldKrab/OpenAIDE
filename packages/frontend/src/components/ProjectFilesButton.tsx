import { GitCompareArrows } from "lucide-react";
import { createPortal } from "react-dom";

/** The Task's way into Project files. It shows how much the workspace has changed, so the user
 * can tell there is something to review without opening the panel. In a compact App Shell
 * header it becomes an icon with a count. */
export function ProjectFilesButton({ added = 0, count = 0, open, onToggle, removed = 0, target }: {
  added?: number;
  /** Changed files in the selected workspace. */
  count?: number;
  open: boolean;
  onToggle(): void;
  removed?: number;
  target?: HTMLElement | null;
}) {
  const label = count
    ? `Project files, ${count} changed ${count === 1 ? "file" : "files"}, ${added} added, ${removed} removed`
    : "Project files";
  const button = (
    <button type="button" className="project-files-entry" aria-label={label} title="Project files" aria-expanded={open} onClick={onToggle}>
      <GitCompareArrows aria-hidden="true" size={target ? 18 : 15} />
      {target ? (
        count > 0 && <span className="project-files-entry-badge">{count > 99 ? "99+" : count}</span>
      ) : count ? (
        <>
          <span>{count} {count === 1 ? "file" : "files"}</span>
          <span className="project-files-added">+{added}</span>
          <span className="project-files-removed">−{removed}</span>
        </>
      ) : (
        <span>Files</span>
      )}
    </button>
  );
  return target ? createPortal(button, target) : button;
}
