import type {
  DesktopUpdateCapability,
  DesktopUpdateSnapshot,
} from "../../../packages/frontend/src/services/frontendShell";

export type AndroidUpdateCommand =
  | { action: "update_check" | "update_download" | "update_cancel" | "update_install" }
  | { action: "update_notes"; version: string };

const initialSnapshot: DesktopUpdateSnapshot = {
  revision: -1,
  installedVersion: "",
  kind: "unavailable",
  unavailableReason: "notConfigured",
};

/**
 * The Android App's own updates over its connection-controls channel. The app owns
 * checking, download, verification, and installation; this keeps its newest snapshot
 * and forwards the user's taps. Android shows its own installer, so nothing here
 * stops the App Server: it runs in Termux or on the paired computer and keeps going.
 */
export function androidAppUpdates(execute: (command: AndroidUpdateCommand) => Promise<void>): {
  capability: DesktopUpdateCapability;
  receive(snapshot: DesktopUpdateSnapshot): void;
} {
  let current = initialSnapshot;
  const listeners = new Set<() => void>();
  return {
    receive(snapshot) {
      if (typeof snapshot?.revision !== "number" || snapshot.revision <= current.revision) return;
      current = snapshot;
      listeners.forEach(listener => listener());
    },
    capability: {
      installation: "systemInstaller",
      snapshot: () => current,
      subscribe(listener) {
        listeners.add(listener);
        return () => { listeners.delete(listener); };
      },
      check: () => execute({ action: "update_check" }),
      download: () => execute({ action: "update_download" }),
      cancelDownload: () => execute({ action: "update_cancel" }),
      async restartAndUpdate() {
        await execute({ action: "update_install" });
        return "started";
      },
      openReleaseNotes: version => execute({ action: "update_notes", version }),
    },
  };
}
