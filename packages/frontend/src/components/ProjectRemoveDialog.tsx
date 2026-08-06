import { Trash2, X } from "lucide-react";
import { useState } from "react";

export function ProjectRemoveDialog({
  onCancel,
  onConfirm,
  project,
}: {
  onCancel: () => void;
  onConfirm: () => Promise<void>;
  project: { label: string; taskCount: number };
}) {
  const [removing, setRemoving] = useState(false);
  const [error, setError] = useState<string>();
  const remove = () => {
    setRemoving(true);
    setError(undefined);
    void onConfirm().catch((cause: unknown) => {
      setRemoving(false);
      setError(cause instanceof Error ? cause.message : "Unable to remove Project.");
    });
  };
  return <div className="project-remove-backdrop">
    <section aria-label={`Remove ${project.label}`} aria-modal="true" className="project-remove-dialog" role="dialog">
      <header><span><Trash2 size={16} /></span><button aria-label="Close" disabled={removing} onClick={onCancel} type="button"><X size={15} /></button></header>
      <h2>Remove {project.label}?</h2>
      {project.taskCount > 0 ? <p>
        {project.taskCount} {project.taskCount === 1 ? "Task" : "Tasks"} will be removed from OpenAIDE. They will remain in their original Agents.
      </p> : <p>This Project will be removed from OpenAIDE.</p>}
      {error ? <small role="alert">{error}</small> : null}
      <footer>
        <button disabled={removing} onClick={onCancel} type="button">Cancel</button>
        <button className="danger" disabled={removing} onClick={remove} type="button">{removing ? "Removing…" : "Remove Project"}</button>
      </footer>
    </section>
  </div>;
}
