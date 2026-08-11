type TaskSessionReloadNoticeProps = {
  error?: string;
  onReload: () => Promise<void>;
  pending: boolean;
};

/** Makes a possible external Native Session change explicit without locking the Composer. */
export function TaskSessionReloadNotice({
  error,
  onReload,
  pending,
}: TaskSessionReloadNoticeProps) {
  return (
    <section className="task-session-reload-notice" role="status">
      <span>
        <strong>This Task may have changed elsewhere.</strong>
        <small>Reload to refresh Chat and Agent options.</small>
        {error ? <small className="task-session-reload-error" role="alert">{error}</small> : null}
      </span>
      <button
        aria-label="Reload Task from Agent"
        disabled={pending}
        onClick={() => void onReload()}
        type="button"
      >
        {pending ? "Reloading" : "Reload"}
      </button>
    </section>
  );
}
