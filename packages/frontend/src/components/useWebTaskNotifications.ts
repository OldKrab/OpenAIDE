import { useEffect, useSyncExternalStore } from "react";
import type { AppController } from "./appController";
import { currentFrontendShell } from "../services/frontendShell";

const subscribeUnavailable = () => () => undefined;
const unavailableSnapshot = () => undefined;

/** Bridges durable Task Attention Events into the optional Web App Shell capability. */
export function useWebTaskNotifications(controller: AppController) {
  const manager = currentFrontendShell()?.taskNotifications;
  const settings = useSyncExternalStore(
    manager?.subscribe ?? subscribeUnavailable,
    manager?.getSettings ?? unavailableSnapshot,
    manager?.getSettings ?? unavailableSnapshot,
  );
  const stateRootId = controller.taskNotifications?.stateRootId;
  const tasks = controller.taskNotifications?.tasks;

  useEffect(() => {
    if (!manager || !controller.backendReady || !stateRootId || !tasks) return;
    manager.reconcile(stateRootId, tasks);
  }, [controller.backendReady, manager, stateRootId, tasks]);

  if (!manager || !settings) return undefined;
  return {
    settings,
    setEnabled: (enabled: boolean) => manager.setEnabled(enabled),
  };
}
