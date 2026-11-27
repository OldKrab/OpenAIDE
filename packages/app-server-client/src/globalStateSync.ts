import type { BackendSyncListener } from "./backendConnection.js";
import type { DiagnosticsLogger } from "./diagnostics.js";
import { applyGlobalStateChanges } from "./globalState.js";
import type { GlobalStateChanges, InitializeResult } from "./generated/protocol.js";

/**
 * Keeps the global state of the current initialization current (ADR-0086). The state is in the
 * `client/initialize` reply; each `sync` notification after it replaces the areas it names. A
 * change can arrive in the same transport batch as the reply, before the reply's continuation has
 * run, and a recovering session has no reply yet: those changes wait and are applied to the
 * result as it is installed, so what a caller receives already includes them.
 */
export function createGlobalStateSync(logger: DiagnosticsLogger) {
  const listeners = new Set<BackendSyncListener>();
  let pending: GlobalStateChanges[] = [];
  let current: InitializeResult | undefined;
  return {
    /** Starts the initialization `result` and returns it with any waiting changes applied. */
    install(result: InitializeResult): InitializeResult {
      const waiting = pending.length;
      current = pending.reduce(withChanges, result);
      pending = [];
      if (waiting > 0) logger.info("app_server_global_sync_applied_to_baseline", { change_count: waiting });
      return current;
    },
    /** Drops the installed result: its transport is gone and the next one brings a new state. */
    suspend() {
      current = undefined;
      pending = [];
    },
    /** The installed result with every change received since, or undefined before one is. */
    latest(): InitializeResult | undefined {
      return current;
    },
    receive(changes: GlobalStateChanges) {
      if (!current) {
        pending.push(changes);
        return;
      }
      current = withChanges(current, changes);
      for (const listener of [...listeners]) {
        try {
          listener(changes);
        } catch (error) {
          logger.error("app_server_global_sync_listener_failed", {
            error_name: error instanceof Error ? error.name : typeof error,
          });
        }
      }
    },
    subscribe(listener: BackendSyncListener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    clear() {
      listeners.clear();
      pending = [];
      current = undefined;
    },
  };
}

function withChanges(result: InitializeResult, changes: GlobalStateChanges): InitializeResult {
  return {
    ...result,
    snapshot: { ...result.snapshot, global: applyGlobalStateChanges(result.snapshot.global, changes) },
  };
}
