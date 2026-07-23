import * as vscode from "vscode";
import type {
  AppServerStateObserver,
  BackendUnsubscribe,
  ProjectId,
  SubscriptionScope,
} from "@openaide/app-server-client";
import { createTaskNotificationManager } from "./taskNotificationManager";
import { workspaceRoots } from "../workspace/roots";

const HANDLED_EVENTS_KEY = "openaide.taskNotifications.handled";

type TaskNotificationRuntime = {
  subscribeAppServerState(
    scope: SubscriptionScope,
    observer: AppServerStateObserver,
  ): Promise<BackendUnsubscribe>;
};

type TaskNotificationState = Pick<vscode.Memento, "get" | "update">;
type TaskSurface = {
  openTask(taskId: string, title?: string): void;
  currentFocusedTaskId(): string | undefined;
  onDidChangeFocusedTask(
    listener: (taskId: string | undefined) => void,
  ): vscode.Disposable;
};
type TaskNotificationLogger = {
  info(message: string, fields?: Record<string, unknown>): void;
  warn(message: string, fields?: Record<string, unknown>): void;
};

/** Connects authoritative Task Attention state to the VS Code notification UI. */
export async function registerTaskNotifications(
  runtime: TaskNotificationRuntime,
  globalState: TaskNotificationState,
  surfaces: TaskSurface,
  logger: TaskNotificationLogger,
): Promise<vscode.Disposable> {
  const manager = createTaskNotificationManager({
    now: () => Date.now(),
    focusedTaskId: () => surfaces.currentFocusedTaskId(),
    readHandledEventIds: () => globalState.get<string[]>(HANDLED_EVENTS_KEY, []),
    rememberHandledEventIds: (eventIds) => {
      void globalState.update(HANDLED_EVENTS_KEY, eventIds).then(undefined, (error) => {
        logger.warn("failed to persist VS Code Task notification receipts", {
          error: error instanceof Error ? error.message : String(error),
        });
      });
    },
    showNotification: async (message, action) => {
      logger.info("showing VS Code Task notification");
      return vscode.window.showInformationMessage(message, action);
    },
    openTask: (taskId, title) => surfaces.openTask(taskId, title),
    subscribeFocusedTask(listener) {
      const subscription = surfaces.onDidChangeFocusedTask(listener);
      return () => subscription.dispose();
    },
    reportError: (error) => {
      logger.warn("VS Code Task notification failed", {
        error: error instanceof Error ? error.message : String(error),
      });
    },
  });
  let stop: BackendUnsubscribe;
  try {
    stop = await runtime.subscribeAppServerState(
      {
        kind: "taskNavigation",
        section: "tasks",
        projectIds: workspaceRoots().map((root) => root.projectId as ProjectId),
      },
      {
        onSnapshot(snapshot) {
          if (snapshot.kind !== "taskNavigation") return;
          manager.reconcile(snapshot.navigation.groups.flatMap((group) => group.entries).flatMap((entry) => (
            entry.kind === "task" ? [entry.task] : []
          )));
        },
        onBaselineError(error) {
          logger.warn("VS Code Task notification subscription failed", {
            error: error instanceof Error ? error.message : String(error),
          });
        },
      },
    );
  } catch (error) {
    manager.dispose();
    throw error;
  }
  return {
    dispose() {
      stop();
      manager.dispose();
    },
  };
}
