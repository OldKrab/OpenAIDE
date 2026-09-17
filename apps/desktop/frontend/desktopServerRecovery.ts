type Invoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

const ENSURE_DEBOUNCE_MS = 1_000;

/**
 * Brings the Desktop-owned App Server back when the machine resumes or the process
 * exited while the window stayed open. The App Server is allowed to shut down while
 * idle, so returning to the app must be able to start it again without a reload.
 * A replacement endpoint is published by the host and picked up by the client.
 */
export function createDesktopServerRecovery({
  invoke,
  target = window,
  documentTarget = document,
}: {
  invoke: Invoke;
  target?: Window;
  documentTarget?: Document;
}) {
  let inFlight = false;
  let lastAttemptAt = 0;

  const ensure = () => {
    if (inFlight || documentTarget.visibilityState === "hidden") return;
    const now = Date.now();
    if (now - lastAttemptAt < ENSURE_DEBOUNCE_MS) return;
    lastAttemptAt = now;
    inFlight = true;
    const operationId = `desktop-ensure-app-server-${crypto.randomUUID()}`;
    const startedAt = performance.now();
    console.info(`desktop_app_server_ensure_requested operation_id=${operationId}`);
    void invoke("desktop_ensure_app_server")
      .then((result) => {
        console.info(
          `desktop_app_server_ensure_settled operation_id=${operationId} outcome=success relaunched=${result !== null} duration_ms=${Math.round(performance.now() - startedAt)}`,
        );
      })
      .catch(() => {
        console.warn(
          `desktop_app_server_ensure_settled operation_id=${operationId} outcome=failure error_kind=ensure_retryable duration_ms=${Math.round(performance.now() - startedAt)}`,
        );
      })
      .finally(() => {
        inFlight = false;
      });
  };

  // Machine sleep and window restore both surface as focus, page show, or a
  // visibility flip; the shell owns the native process, so it re-checks here.
  const onVisibilityChange = () => ensure();
  target.addEventListener("focus", ensure);
  target.addEventListener("pageshow", ensure);
  target.addEventListener("openaide:resume", ensure);
  documentTarget.addEventListener("visibilitychange", onVisibilityChange);
  return () => {
    target.removeEventListener("focus", ensure);
    target.removeEventListener("pageshow", ensure);
    target.removeEventListener("openaide:resume", ensure);
    documentTarget.removeEventListener("visibilitychange", onVisibilityChange);
  };
}
