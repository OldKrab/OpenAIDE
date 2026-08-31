import type { InvokeArgs } from "@tauri-apps/api/core";
import type { Event, UnlistenFn } from "@tauri-apps/api/event";
import type { AppServerSession } from "@openaide/app-server-client";
import {
  CLIENT_DETACH,
  CLIENT_UPDATE_SHUTDOWN_ABORT,
  CLIENT_UPDATE_SHUTDOWN_COMMIT,
  CLIENT_UPDATE_SHUTDOWN_PREPARE,
} from "@openaide/app-server-client";
import type {
  DesktopUpdateCapability,
  DesktopUpdateSnapshot,
} from "../../../packages/frontend/src/services/frontendShell";

type Invoke = (command: string, args?: InvokeArgs) => Promise<DesktopUpdateSnapshot>;
type Listen = <T>(event: string, handler: (event: Event<T>) => void) => Promise<UnlistenFn>;

const initialSnapshot: DesktopUpdateSnapshot = {
  revision: -1,
  installedVersion: "",
  kind: "unavailable",
  unavailableReason: "notConfigured",
};
const FINAL_DETACH_RESPONSE_TIMEOUT_MS = 1_000;

type FinalDetachOutcome = "response" | "transportClosed" | "timeout";

/** The App Server may exit after accepting its final detach before its response is observable. */
async function waitForFinalDetach(request: Promise<unknown>): Promise<FinalDetachOutcome> {
  let timeout: ReturnType<typeof setTimeout> | undefined;
  const deadline = new Promise<FinalDetachOutcome>((resolve) => {
    timeout = setTimeout(() => resolve("timeout"), FINAL_DETACH_RESPONSE_TIMEOUT_MS);
  });
  const response = request.then<FinalDetachOutcome, FinalDetachOutcome>(
    () => "response",
    () => "transportClosed",
  );
  const outcome = await Promise.race([response, deadline]);
  if (timeout !== undefined) clearTimeout(timeout);
  return outcome;
}

/** Adapts native revisioned snapshots without exposing Tauri's generic updater API. */
export function createDesktopUpdates({
  invoke,
  listen,
  openReleaseNotes,
  reload,
  session,
}: {
  invoke: Invoke;
  listen: Listen;
  openReleaseNotes(version: string): void;
  reload(): void;
  session: AppServerSession;
}): DesktopUpdateCapability {
  let current = initialSnapshot;
  const listeners = new Set<() => void>();
  const publish = (next: DesktopUpdateSnapshot) => {
    if (next.revision <= current.revision) return;
    current = next;
    for (const listener of listeners) listener();
  };
  const run = async (command: string) => {
    publish(await invoke(command));
  };

  let disposed = false;
  let stop: UnlistenFn | undefined;
  void listen<DesktopUpdateSnapshot>("desktop-update-snapshot", ({ payload }) => publish(payload))
    .then((unsubscribe) => {
      if (disposed) unsubscribe();
      else stop = unsubscribe;
    });
  void run("desktop_update_snapshot");

  return {
    snapshot: () => current,
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
        if (listeners.size === 0 && disposed) stop?.();
      };
    },
    check: () => run("desktop_check_for_update"),
    download: () => run("desktop_download_update"),
    cancelDownload: () => run("desktop_cancel_update_download"),
    async restartAndUpdate(options) {
      const attemptId = `desktop-update-${Date.now()}-${Math.random().toString(36).slice(2)}`;
      let prepared = false;
      let detached = false;
      try {
        const readiness = await session.request(CLIENT_UPDATE_SHUTDOWN_PREPARE, {
          attemptId,
          stopActiveWork: options?.stopActiveWork ?? false,
        });
        if (readiness.kind === "blocked") return readiness.reason;
        prepared = true;
        await session.request(CLIENT_UPDATE_SHUTDOWN_COMMIT, { attemptId });
        const detachStartedAt = performance.now();
        console.info(`desktop_update_detach_started operation_id=${attemptId}`);
        const detachRequest = session.request(CLIENT_DETACH, {});
        // Dispatching detach commits the point of no return. The native command
        // remains authoritative for proving that the listener actually stopped.
        detached = true;
        const detachOutcome = await waitForFinalDetach(detachRequest);
        console.info(
          `desktop_update_detach_completed operation_id=${attemptId} outcome=${detachOutcome} duration_ms=${Math.round(performance.now() - detachStartedAt)}`,
        );
        await session.close();
        await run("desktop_install_update");
        return "started";
      } catch (error) {
        if (prepared && !detached) {
          try {
            await session.request(CLIENT_UPDATE_SHUTDOWN_ABORT, { attemptId });
          } catch {
            // The original failure remains authoritative; a live client can recover on reload.
          }
        }
        // Once shutdown starts, reload is the only safe way to restore the
        // logical Desktop client if installer handoff does not occur.
        if (detached) reload();
        throw error;
      }
    },
    openReleaseNotes,
  };
}
