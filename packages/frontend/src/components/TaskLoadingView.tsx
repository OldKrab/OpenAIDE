import type { TaskOpenError } from "../state/store";

/** Placeholder for a Task that is opening or failed to open, with its retry. */
export function TaskLoadingView({
  error,
  errorKind,
  label = "Opening task",
  onRetry,
}: {
  error?: string;
  errorKind?: TaskOpenError["kind"];
  label?: string;
  onRetry?: () => void;
}) {
  if (error) {
    if (errorKind === "conflict") {
      return (
        <section className="task-surface task-loading" aria-label="Session open elsewhere">
          <p>Session open elsewhere.</p>
          <small className="inline-error" role="alert">{error}</small>
          {onRetry ? <button type="button" onClick={onRetry}>Try again</button> : null}
        </section>
      );
    }
    if (errorKind === "notFound") {
      return (
        <section className="task-surface task-loading" aria-label="Task not found">
          <p>Task not found.</p>
          <small className="inline-error" role="alert">
            This Task does not exist or is no longer available.
          </small>
        </section>
      );
    }
    return (
      <section className="task-surface task-loading" aria-label="Unable to open task">
        <p>Unable to open task.</p>
        <small className="inline-error" role="alert">
          {error}
        </small>
        {onRetry ? <button type="button" onClick={onRetry}>Retry</button> : null}
      </section>
    );
  }
  return (
    <section className="task-surface task-loading" aria-label={label}>
      <div className="task-loading-status" role="status" aria-live="polite">
        <span className="working-status-dots" aria-hidden="true">
          <span />
          <span />
          <span />
        </span>
        <span>{label}</span>
      </div>
    </section>
  );
}
