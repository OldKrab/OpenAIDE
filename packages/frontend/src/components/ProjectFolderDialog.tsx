import { X } from "lucide-react";
import { useState } from "react";

import type { WorkspaceBrowserCallbacks } from "./appControllerCallbackTypes";
import { NewWorkspacePicker } from "./NewWorkspacePicker";

/** Web-owned Project picker; native shells provide their own folder dialog. */
export function ProjectFolderDialog({
  browser,
  onClose,
  onSelect,
}: {
  browser: WorkspaceBrowserCallbacks;
  onClose: () => void;
  onSelect: (folder: { path: string; label: string }) => Promise<void>;
}) {
  const [error, setError] = useState<string>();
  const [submitting, setSubmitting] = useState(false);
  const select = (folder: { path: string; label: string }) => {
    setSubmitting(true);
    setError(undefined);
    void onSelect(folder).catch((cause: unknown) => {
      setSubmitting(false);
      setError(cause instanceof Error ? cause.message : "Unable to add Project.");
    });
  };
  return (
    <div className="project-folder-backdrop" onMouseDown={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}>
      <section aria-label="Add Project" aria-modal="true" className="project-folder-dialog" role="dialog">
        <header>
          <span>
            <strong>Add Project</strong>
            <small>Browse to the Project folder, or edit the path directly.</small>
          </span>
          <button aria-label="Close Add Project" disabled={submitting} onClick={onClose} type="button"><X size={15} /></button>
        </header>
        {error ? <p className="project-folder-error" role="alert">{error}</p> : null}
        <div aria-busy={submitting || undefined} className={submitting ? "project-folder-submitting" : undefined}>
          <NewWorkspacePicker browser={browser} onSelect={select} />
        </div>
      </section>
    </div>
  );
}
