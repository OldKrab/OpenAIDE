import type { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { ComputerSwitchResult, ComputersCapability, ComputersSnapshot, ShellComputer, SshComputerStatus } from "../../../packages/frontend/src/services/computers";

/** What the native side reports for a switch it was asked for. */
type NativeSwitchOutcome = "switched" | "alreadyInUse" | "cancelled";

/**
 * The Desktop's paired computers (ADR-0063) and SSH Computers (ADR-0069), kept by the native side in its
 * bootstrap preferences. The native side connects the chosen computer first and
 * keeps this computer's App Server running (ADR-0070); once it switched, the
 * window leaves its App Server and reloads onto the new one.
 */
export function createDesktopComputers(
  invoke: typeof tauriInvoke,
  /** Leaves the App Server in use and reloads the window onto the one now chosen. */
  reload: () => Promise<void>,
): ComputersCapability {
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
    async use(id, operationId) {
      return settle(await invoke<NativeSwitchOutcome>("desktop_remote_use", { id: id ?? null, operationId: operationId ?? null }));
    },
    cancel: async () => {
      await invoke<boolean>("desktop_remote_cancel");
    },
    async forget(id) {
      // Forgetting the computer in use moves the window to this computer first.
      if (await settle(await invoke<NativeSwitchOutcome>("desktop_remote_forget", { id, operationId: null })) !== "switched") await read();
    },
    async pair(code) {
      if (await settle(await invoke<NativeSwitchOutcome>("desktop_remote_pair", { code, operationId: null })) !== "switched") await read();
    },
    // SSH Computers (ADR-0069) are rows of the same list, marked by their `ssh` destination.
    ssh: {
      hosts: () => invoke<string[]>("desktop_ssh_hosts"),
      async add(destination, name) {
        const result = await settle(await invoke<NativeSwitchOutcome>("desktop_ssh_add", { destination, name: name ?? null, operationId: null }));
        if (result !== "switched") await read();
        return result;
      },
      status: async () => (await invoke<SshComputerStatus | null>("desktop_ssh_status")) ?? undefined,
      update: () => invoke("desktop_ssh_update"),
      stop: (id) => invoke<"stopped" | "notRunning" | "stillRunning">("desktop_ssh_stop", { id }),
    },
  };

  async function settle(outcome: NativeSwitchOutcome): Promise<ComputerSwitchResult> {
    if (outcome === "alreadyInUse") return "unchanged";
    if (outcome === "switched") await reload();
    return outcome;
  }
}
