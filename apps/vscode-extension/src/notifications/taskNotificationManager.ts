import type { TaskAttentionReason, TaskSummary } from "@openaide/app-server-client";

const OPEN_TASK_ACTION = "Open Task";
const MAX_HANDLED_EVENTS = 500;

export type TaskNotificationEnvironment = {
  now(): number;
  isFocused(): boolean;
  focusedTaskId(): string | undefined;
  readHandledEventIds(): string[];
  rememberHandledEventIds(eventIds: string[]): void;
  showNotification(message: string, action: string): Promise<string | undefined>;
  openTask(taskId: string, title: string): void;
  subscribeFocus(listener: (focused: boolean) => void): () => void;
  subscribeFocusedTask(listener: (taskId: string | undefined) => void): () => void;
  reportError?(error: unknown): void;
};

export type TaskNotificationManager = {
  reconcile(tasks: TaskSummary[]): void;
  dispose(): void;
};

/** Owns VS Code-window focus eligibility and durable Task Attention receipts. */
export function createTaskNotificationManager(
  environment: TaskNotificationEnvironment,
): TaskNotificationManager {
  const handled = new Set(environment.readHandledEventIds());
  let baselineInstalled = false;
  let disposed = false;
  let focusedTaskId = currentFocusedTaskId();
  let focusedSince = focusedTaskId ? environment.now() : undefined;
  const lastFocusedIntervals = new Map<string, { from: number; until: number }>();

  const stopFocus = environment.subscribeFocus(updateFocusedTask);
  const stopFocusedTask = environment.subscribeFocusedTask(updateFocusedTask);

  return {
    reconcile(tasks) {
      if (disposed) return;
      if (!baselineInstalled) {
        baselineInstalled = true;
        let changed = false;
        for (const task of tasks) {
          if (task.attention) changed = rememberHandled(task.attention.eventId, false) || changed;
        }
        if (changed) persistHandled();
        return;
      }
      for (const task of tasks) reconcileTask(task);
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      stopFocus();
      stopFocusedTask();
    },
  };

  function reconcileTask(task: TaskSummary) {
    const attention = task.attention;
    if (!attention || handled.has(attention.eventId)) return;

    // Claim before showing so repeated snapshots cannot duplicate an active toast.
    rememberHandled(attention.eventId);
    const occurredAt = notificationTimestamp(attention.occurredAt);
    if (!Number.isFinite(occurredAt) || wasTaskFocusedAt(task.taskId, occurredAt)) return;

    const title = task.title?.value.trim() || "Untitled task";
    void environment.showNotification(
      attentionMessage(attention.reason, title),
      OPEN_TASK_ACTION,
    ).then((selection) => {
      if (!disposed && selection === OPEN_TASK_ACTION) {
        environment.openTask(task.taskId, title);
      }
    }).catch((error) => environment.reportError?.(error));
  }

  function updateFocusedTask() {
    const nextTaskId = currentFocusedTaskId();
    if (nextTaskId === focusedTaskId) return;
    const changedAt = environment.now();
    if (focusedTaskId && focusedSince !== undefined) {
      lastFocusedIntervals.set(focusedTaskId, { from: focusedSince, until: changedAt });
    }
    focusedTaskId = nextTaskId;
    focusedSince = nextTaskId ? changedAt : undefined;
  }

  function currentFocusedTaskId() {
    return environment.isFocused() ? environment.focusedTaskId() : undefined;
  }

  function wasTaskFocusedAt(taskId: string, occurredAt: number) {
    const currentFocusIncludesEvent = focusedTaskId === taskId
      && focusedSince !== undefined
      && focusedSince <= occurredAt;
    const previous = lastFocusedIntervals.get(taskId);
    return currentFocusIncludesEvent
      || focusIntervalIncludes(previous?.from, previous?.until, occurredAt);
  }

  function rememberHandled(eventId: string, persist = true) {
    if (handled.has(eventId)) return false;
    handled.add(eventId);
    while (handled.size > MAX_HANDLED_EVENTS) {
      const oldest = handled.values().next().value;
      if (typeof oldest !== "string") break;
      handled.delete(oldest);
    }
    if (persist) persistHandled();
    return true;
  }

  function persistHandled() {
    environment.rememberHandledEventIds([...handled]);
  }
}

/** App Server persists epoch milliseconds while tests and transports may use ISO timestamps. */
function notificationTimestamp(value: string) {
  const trimmed = value.trim();
  if (/^\d+$/.test(trimmed)) return Number(trimmed);
  return Date.parse(trimmed);
}

function focusIntervalIncludes(
  from: number | undefined,
  until: number | undefined,
  occurredAt: number,
) {
  return from !== undefined
    && until !== undefined
    && from <= occurredAt
    && occurredAt <= until;
}

function attentionMessage(reason: TaskAttentionReason, title: string) {
  switch (reason) {
    case "finished":
      return `Task finished: ${title}`;
    case "needsPermission":
      return `Task needs permission: ${title}`;
    case "needsAnswer":
      return `Task needs an answer: ${title}`;
    case "stopped":
      return `Task stopped: ${title}`;
    case "failed":
      return `Task failed: ${title}`;
  }
}
