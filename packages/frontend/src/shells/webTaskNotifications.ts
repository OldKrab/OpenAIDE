import type { TaskSummary } from "@openaide/app-shell-contracts";

const ENABLED_KEY = "openaide.desktopNotifications.enabled";
const ENABLED_AT_KEY = "openaide.desktopNotifications.enabledAt";
const HANDLED_KEY = "openaide.desktopNotifications.handled";
const MAX_HANDLED_EVENTS = 500;
const PRESENCE_STALE_AFTER_MS = 30_000;

export type DesktopNotificationSettings = {
  status: "off" | "enabled" | "blocked" | "unsupported";
};

type CoordinationMessage =
  | { type: "hello"; tabId: string }
  | {
      type: "presence";
      tabId: string;
      focused: boolean;
      focusedSince?: number;
      lastFocusedFrom?: number;
      lastFocusedUntil?: number;
      sentAt: number;
    }
  | { type: "handled"; tabId: string; eventId: string }
  | { type: "settings"; tabId: string };

export type WebTaskNotificationEnvironment = {
  storage: Pick<Storage, "getItem" | "setItem" | "removeItem">;
  notificationIconUrl: string;
  now(): number;
  isFocused(): boolean;
  notificationsSupported(): boolean;
  notificationPermission(): NotificationPermission;
  requestNotificationPermission(): Promise<NotificationPermission>;
  showNotification(
    title: string,
    options: NotificationOptions,
    onClick: () => void,
  ): { close(): void };
  focusWindow(): void;
  openTask(taskId: string, title: string): void;
  subscribeFocus(listener: () => void): () => void;
  publish(message: CoordinationMessage): void;
  subscribeMessages(listener: (message: unknown) => void): () => void;
};

export type WebTaskNotificationManager = {
  getSettings(): DesktopNotificationSettings;
  subscribe(listener: () => void): () => void;
  setEnabled(enabled: boolean): Promise<void>;
  reconcile(stateRootId: string, tasks: TaskSummary[]): void;
  dispose(): void;
};

/** Owns browser-local permission, focus coordination, receipts, and OS presentation. */
export function createWebTaskNotificationManager(
  environment: WebTaskNotificationEnvironment,
): WebTaskNotificationManager {
  const tabId = createTabId(environment.now());
  const listeners = new Set<() => void>();
  const peers = new Map<string, {
    focused: boolean;
    focusedSince?: number;
    lastFocusedFrom?: number;
    lastFocusedUntil?: number;
    observedAt: number;
  }>();
  const handled = readHandled(environment.storage);
  const startupBaseline = new Set<string>();
  const active = new Map<string, { eventId: string; notification: { close(): void } }>();
  let stateRootId: string | undefined;
  let baselineInstalled = false;
  let settings = deriveSettings(environment);
  let disposed = false;
  let focusedSince = environment.isFocused() ? environment.now() : undefined;
  let lastFocusedFrom: number | undefined;
  let lastFocusedUntil: number | undefined;

  const publishPresence = () => {
    environment.publish({
      type: "presence",
      tabId,
      focused: environment.isFocused(),
      focusedSince,
      lastFocusedFrom,
      lastFocusedUntil,
      sentAt: environment.now(),
    });
  };
  const stopFocus = environment.subscribeFocus(() => {
    const focused = environment.isFocused();
    if (focused && focusedSince === undefined) focusedSince = environment.now();
    if (!focused && focusedSince !== undefined) {
      lastFocusedFrom = focusedSince;
      lastFocusedUntil = environment.now();
      focusedSince = undefined;
    }
    refreshSettings();
    publishPresence();
  });
  const stopMessages = environment.subscribeMessages((value) => {
    const message = coordinationMessage(value);
    if (!message || message.tabId === tabId) return;
    if (message.type === "hello") {
      publishPresence();
      return;
    }
    if (message.type === "presence") {
      peers.set(message.tabId, {
        focused: message.focused,
        focusedSince: message.focusedSince,
        lastFocusedFrom: message.lastFocusedFrom,
        lastFocusedUntil: message.lastFocusedUntil,
        observedAt: environment.now(),
      });
      return;
    }
    if (message.type === "handled") {
      rememberHandled(message.eventId);
      return;
    }
    refreshSettings();
  });
  environment.publish({ type: "hello", tabId });
  publishPresence();

  return {
    getSettings: () => settings,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    async setEnabled(enabled) {
      if (!enabled) {
        environment.storage.removeItem(ENABLED_KEY);
        environment.storage.removeItem(ENABLED_AT_KEY);
        closeAll();
        refreshSettings();
        environment.publish({ type: "settings", tabId });
        return;
      }
      environment.storage.setItem(ENABLED_KEY, "true");
      if (!environment.storage.getItem(ENABLED_AT_KEY)) {
        environment.storage.setItem(ENABLED_AT_KEY, new Date(environment.now()).toISOString());
      }
      if (
        environment.notificationsSupported()
        && environment.notificationPermission() !== "granted"
      ) {
        await environment.requestNotificationPermission();
      }
      refreshSettings();
      environment.publish({ type: "settings", tabId });
    },
    reconcile(nextStateRootId, tasks) {
      if (disposed) return;
      refreshSettings();
      if (stateRootId !== nextStateRootId) {
        stateRootId = nextStateRootId;
        baselineInstalled = false;
        startupBaseline.clear();
        closeAll();
      }
      closeStaleNotifications(tasks);
      if (!baselineInstalled) {
        baselineInstalled = true;
        for (const task of tasks) {
          if (task.attention) {
            startupBaseline.add(task.attention.event_id);
            rememberHandled(task.attention.event_id);
          }
        }
        return;
      }
      for (const task of tasks) reconcileTask(task);
    },
    dispose() {
      disposed = true;
      stopFocus();
      stopMessages();
      closeAll();
      listeners.clear();
      peers.clear();
    },
  };

  function reconcileTask(task: TaskSummary) {
    const event = task.attention;
    if (!event || startupBaseline.has(event.event_id) || handled.has(event.event_id)) return;
    if (settings.status !== "enabled") {
      rememberAndPublish(event.event_id);
      return;
    }
    const enabledAt = notificationTimestamp(environment.storage.getItem(ENABLED_AT_KEY) ?? "");
    const occurredAt = notificationTimestamp(event.occurred_at);
    if (!Number.isFinite(occurredAt) || !Number.isFinite(enabledAt) || occurredAt < enabledAt) {
      rememberAndPublish(event.event_id);
      return;
    }
    if (wasFocusedAt(occurredAt)) {
      rememberAndPublish(event.event_id);
      return;
    }

    // Claim before display so synchronous tab coordination cannot double-deliver the event.
    rememberAndPublish(event.event_id);
    active.get(task.task_id)?.notification.close();
    let notification: { close(): void } | undefined;
    notification = environment.showNotification(
      attentionNotificationTitle(event.reason),
      {
        body: task.title,
        icon: environment.notificationIconUrl,
        tag: `openaide-task-${task.task_id}`,
      },
      () => {
        notification?.close();
        active.delete(task.task_id);
        environment.openTask(task.task_id, task.title);
        // React commits the new route on the next turn. Focusing earlier lets the
        // previous Task's focus receipt mark that Task read before it unmounts.
        setTimeout(() => {
          if (!disposed) environment.focusWindow();
        }, 0);
      },
    );
    active.set(task.task_id, { eventId: event.event_id, notification });
  }

  function closeStaleNotifications(tasks: TaskSummary[]) {
    const current = new Map(tasks.map((task) => [task.task_id, task.attention?.event_id]));
    for (const [taskId, shown] of active) {
      if (current.get(taskId) === shown.eventId) continue;
      shown.notification.close();
      active.delete(taskId);
    }
  }

  function rememberAndPublish(eventId: string) {
    rememberHandled(eventId);
    environment.publish({ type: "handled", tabId, eventId });
  }

  function wasFocusedAt(occurredAt: number) {
    const ownFocusIncludesEvent = environment.isFocused()
      && focusedSince !== undefined
      && focusedSince <= occurredAt;
    if (
      ownFocusIncludesEvent
      || focusIntervalIncludes(lastFocusedFrom, lastFocusedUntil, occurredAt)
    ) return true;
    const now = environment.now();
    return [...peers.values()].some((peer) => {
      const currentFocusIncludesEvent = peer.focused
        && now - peer.observedAt <= PRESENCE_STALE_AFTER_MS
        && peer.focusedSince !== undefined
        && peer.focusedSince <= occurredAt;
      return currentFocusIncludesEvent || focusIntervalIncludes(
        peer.lastFocusedFrom,
        peer.lastFocusedUntil,
        occurredAt,
      );
    });
  }

  function rememberHandled(eventId: string) {
    if (handled.has(eventId)) return;
    handled.add(eventId);
    while (handled.size > MAX_HANDLED_EVENTS) {
      const oldest = handled.values().next().value;
      if (typeof oldest !== "string") break;
      handled.delete(oldest);
    }
    environment.storage.setItem(HANDLED_KEY, JSON.stringify([...handled]));
  }

  function refreshSettings() {
    const next = deriveSettings(environment);
    if (next.status === settings.status) return;
    settings = next;
    if (settings.status !== "enabled") closeAll();
    for (const listener of listeners) listener();
  }

  function closeAll() {
    for (const shown of active.values()) shown.notification.close();
    active.clear();
  }
}

function deriveSettings(environment: WebTaskNotificationEnvironment): DesktopNotificationSettings {
  if (!environment.notificationsSupported()) return { status: "unsupported" };
  const requested = environment.storage.getItem(ENABLED_KEY) === "true";
  if (!requested) return { status: "off" };
  return environment.notificationPermission() === "granted"
    ? { status: "enabled" }
    : { status: "blocked" };
}

/** App Server persists epoch milliseconds while browser-local settings use ISO timestamps. */
function notificationTimestamp(value: string) {
  const trimmed = value.trim();
  if (/^\d+$/.test(trimmed)) return Number(trimmed);
  return Date.parse(trimmed);
}

function readHandled(storage: WebTaskNotificationEnvironment["storage"]) {
  try {
    const value = JSON.parse(storage.getItem(HANDLED_KEY) ?? "[]");
    return new Set<string>(Array.isArray(value) ? value.filter((item) => typeof item === "string") : []);
  } catch {
    return new Set<string>();
  }
}

function attentionNotificationTitle(reason: NonNullable<TaskSummary["attention"]>["reason"]) {
  switch (reason) {
    case "finished": return "Task finished";
    case "needsPermission": return "Permission needed";
    case "needsAnswer": return "Answer needed";
    case "stopped": return "Task stopped";
    case "failed": return "Task failed";
  }
}

function coordinationMessage(value: unknown): CoordinationMessage | undefined {
  if (!value || typeof value !== "object") return undefined;
  const message = value as Partial<CoordinationMessage>;
  if (typeof message.tabId !== "string") return undefined;
  if (message.type === "hello" || message.type === "settings") {
    return { type: message.type, tabId: message.tabId };
  }
  if (
    message.type === "presence"
    && typeof message.focused === "boolean"
    && typeof message.sentAt === "number"
    && (message.focusedSince === undefined || typeof message.focusedSince === "number")
    && (message.lastFocusedFrom === undefined || typeof message.lastFocusedFrom === "number")
    && (message.lastFocusedUntil === undefined || typeof message.lastFocusedUntil === "number")
  ) {
    return {
      type: "presence",
      tabId: message.tabId,
      focused: message.focused,
      focusedSince: message.focusedSince,
      lastFocusedFrom: message.lastFocusedFrom,
      lastFocusedUntil: message.lastFocusedUntil,
      sentAt: message.sentAt,
    };
  }
  if (message.type === "handled" && typeof message.eventId === "string") {
    return { type: "handled", tabId: message.tabId, eventId: message.eventId };
  }
  return undefined;
}

function focusIntervalIncludes(from: number | undefined, until: number | undefined, at: number) {
  return from !== undefined && until !== undefined && from <= at && until >= at;
}

function createTabId(now: number) {
  const random = globalThis.crypto?.randomUUID?.() ?? Math.random().toString(36).slice(2);
  return `${now.toString(36)}-${random}`;
}
