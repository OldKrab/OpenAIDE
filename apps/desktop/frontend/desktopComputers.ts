import type { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { ComputersCapability, ComputersSnapshot, ShellComputer } from "../../../packages/frontend/src/services/computers";

/**
 * The Desktop's paired computers (ADR-0063), kept by the native side in its
 * bootstrap preferences. A switch or a pairing that takes effect restarts the
 * Desktop, so their promises settle only when the native side refused.
 */
export function createDesktopComputers(invoke: typeof tauriInvoke): ComputersCapability {
  let snapshot: ComputersSnapshot | undefined;
  let reading: Promise<void> | undefined;
  const listeners = new Set<() => void>();
  const publish = (next: ComputersSnapshot) => {
    snapshot = next;
    listeners.forEach((listener) => listener());
  };
  const read = () => reading ??= (async () => {
    const startedAt = performance.now();
    try {
      publish({ device: "computer", computers: await invoke<ShellComputer[]>("desktop_remote_snapshot") });
    } catch {
      console.warn(`desktop_computers_read_completed outcome=failure duration_ms=${Math.round(performance.now() - startedAt)}`);
      publish({ device: "computer", computers: snapshot?.computers ?? [], error: "OpenAIDE could not read its paired computers. Try again." });
    } finally {
      reading = undefined;
    }
  })();
  void read();
  return {
    snapshot: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      // A failed read is retried when something new starts watching, such as Settings opening.
      if (snapshot?.error) void read();
      return () => listeners.delete(listener);
    },
    use: (id) => invoke("desktop_remote_use", { id: id ?? null }),
    async forget(id) {
      await invoke("desktop_remote_forget", { id });
      await read();
    },
    pair: (code) => invoke("desktop_remote_pair", { code }),
  };
}
