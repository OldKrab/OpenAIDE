import { describe, expect, it, vi } from "vitest";
import type { TaskSummary } from "@openaide/app-shell-contracts";
import {
  createWebTaskNotificationManager,
  type WebTaskNotificationEnvironment,
} from "./webTaskNotifications";

describe("Web Task notifications", () => {
  it("uses the first authoritative task list as a silent startup baseline", () => {
    const environment = testEnvironment({ focused: false, permission: "granted", enabled: true });
    const manager = createWebTaskNotificationManager(environment);

    manager.reconcile("root-1", [task("attention-old", "finished")]);

    expect(environment.notifications).toHaveLength(0);
  });

  it("shows a new unattended event and opens its Task when clicked", () => {
    vi.useFakeTimers();
    try {
      const openTask = vi.fn();
      const focusWindow = vi.fn();
      const environment = testEnvironment({
        focused: false,
        permission: "granted",
        enabled: true,
        focusWindow,
        openTask,
      });
      const manager = createWebTaskNotificationManager(environment);
      manager.reconcile("root-1", []);

      manager.reconcile("root-1", [task("attention-1", "needsPermission")]);

      expect(environment.notifications).toHaveLength(1);
      expect(environment.notifications[0]).toMatchObject({
        title: "Permission needed",
        options: {
          body: "Implement notifications",
          icon: "/openaide.png",
          tag: "openaide-task-task-1",
        },
      });
      environment.notifications[0].click();
      vi.runAllTimers();
      expect(focusWindow).toHaveBeenCalledTimes(1);
      expect(openTask).toHaveBeenCalledWith("task-1", "Implement notifications");
      expect(environment.notifications[0].closed).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it("opens the notified Task before focus recovery can mark the prior Task read", () => {
    vi.useFakeTimers();
    try {
      let routedTaskId = "task-first";
      let pendingTaskId: string | undefined;
      let focusedTaskId: string | undefined;
      const environment = testEnvironment({
        focused: false,
        permission: "granted",
        enabled: true,
        focusWindow: () => { focusedTaskId = routedTaskId; },
        openTask: (taskId) => { pendingTaskId = taskId; },
      });
      const manager = createWebTaskNotificationManager(environment);
      manager.reconcile("root-1", []);
      manager.reconcile("root-1", [task("attention-second", "finished")]);

      environment.notifications[0].click();
      routedTaskId = pendingTaskId ?? routedTaskId;
      vi.runAllTimers();

      expect(focusedTaskId).toBe("task-1");
    } finally {
      vi.useRealTimers();
    }
  });

  it("delivers App Server millisecond-epoch attention timestamps", () => {
    const environment = testEnvironment({ focused: false, permission: "granted", enabled: true });
    const manager = createWebTaskNotificationManager(environment);
    manager.reconcile("root-1", []);
    const completed = task("attention-epoch", "finished");
    completed.attention!.occurred_at = "1784116599266";

    manager.reconcile("root-1", [completed]);

    expect(environment.notifications).toHaveLength(1);
  });

  it("suppresses events that occur while any coordinated tab is focused", () => {
    const shared = sharedBrowserState();
    const firstEnvironment = testEnvironment({
      focused: true,
      permission: "granted",
      enabled: true,
      shared,
    });
    const secondEnvironment = testEnvironment({
      focused: false,
      permission: "granted",
      enabled: true,
      shared,
    });
    const first = createWebTaskNotificationManager(firstEnvironment);
    const second = createWebTaskNotificationManager(secondEnvironment);
    first.reconcile("root-1", []);
    second.reconcile("root-1", []);

    first.reconcile("root-1", [task("attention-1", "finished")]);
    second.reconcile("root-1", [task("attention-1", "finished")]);
    firstEnvironment.setFocused(false);
    second.reconcile("root-1", [task("attention-1", "finished")]);

    expect(firstEnvironment.notifications).toHaveLength(0);
    expect(secondEnvironment.notifications).toHaveLength(0);
  });

  it("deduplicates unattended delivery across coordinated tabs", () => {
    const shared = sharedBrowserState();
    const firstEnvironment = testEnvironment({ focused: false, permission: "granted", enabled: true, shared });
    const secondEnvironment = testEnvironment({ focused: false, permission: "granted", enabled: true, shared });
    const first = createWebTaskNotificationManager(firstEnvironment);
    const second = createWebTaskNotificationManager(secondEnvironment);
    first.reconcile("root-1", []);
    second.reconcile("root-1", []);

    first.reconcile("root-1", [task("attention-1", "finished")]);
    second.reconcile("root-1", [task("attention-1", "finished")]);

    expect(firstEnvironment.notifications.length + secondEnvironment.notifications.length).toBe(1);
  });

  it("delivers a reconnect event that occurred before the page gained focus", () => {
    const environment = testEnvironment({ focused: false, permission: "granted", enabled: true });
    const manager = createWebTaskNotificationManager(environment);
    manager.reconcile("root-1", []);

    environment.setNow("2026-07-15T10:00:01.000Z");
    environment.setFocused(true);
    manager.reconcile("root-1", [task("attention-1", "finished")]);

    expect(environment.notifications).toHaveLength(1);
  });

  it("suppresses a delayed event observed just after the page loses focus", () => {
    const environment = testEnvironment({ focused: true, permission: "granted", enabled: true });
    const manager = createWebTaskNotificationManager(environment);
    manager.reconcile("root-1", []);

    environment.setNow("2026-07-15T10:00:01.000Z");
    environment.setFocused(false);
    manager.reconcile("root-1", [task("attention-1", "finished")]);

    expect(environment.notifications).toHaveLength(0);
  });

  it("replaces and clears the current notification per Task", () => {
    const environment = testEnvironment({ focused: false, permission: "granted", enabled: true });
    const manager = createWebTaskNotificationManager(environment);
    manager.reconcile("root-1", []);
    manager.reconcile("root-1", [task("attention-1", "needsPermission")]);
    const first = environment.notifications[0];

    manager.reconcile("root-1", [task("attention-2", "failed")]);

    expect(first.closed).toBe(true);
    expect(environment.notifications[1]).toMatchObject({
      title: "Task failed",
      options: { body: "Implement notifications" },
    });
    manager.reconcile("root-1", [task(undefined)]);
    expect(environment.notifications[1].closed).toBe(true);
  });

  it("requests permission only when explicitly enabled and exposes blocked state", async () => {
    const environment = testEnvironment({ focused: false, permission: "default", enabled: false });
    const manager = createWebTaskNotificationManager(environment);
    expect(manager.getSettings()).toEqual({ status: "off" });

    environment.permissionRequestResult = "denied";
    await manager.setEnabled(true);

    expect(environment.requestPermission).toHaveBeenCalledTimes(1);
    expect(manager.getSettings()).toEqual({ status: "blocked" });
    await manager.setEnabled(false);
    expect(manager.getSettings()).toEqual({ status: "off" });
  });
});

function task(
  eventId?: string,
  reason: NonNullable<TaskSummary["attention"]>["reason"] = "finished",
): TaskSummary {
  return {
    task_id: "task-1",
    project_id: "project-1",
    title: "Implement notifications",
    status: eventId ? "waiting" : "inactive",
    task_version: 1,
    message_history_version: 1,
    has_messages: true,
    unread: Boolean(eventId),
    attention: eventId ? {
      event_id: eventId,
      reason,
      occurred_at: "2026-07-15T10:00:00.000Z",
    } : undefined,
    created_at: "2026-07-15T09:00:00.000Z",
    updated_at: "2026-07-15T10:00:00.000Z",
    last_activity: "2026-07-15T10:00:00.000Z",
    agent_id: "codex",
    agent_name: "Codex",
    isolation: "local",
    workspace_root: "",
  };
}

type TestNotification = {
  title: string;
  options: NotificationOptions;
  closed: boolean;
  click(): void;
};

function sharedBrowserState() {
  const values = new Map<string, string>();
  const listeners = new Set<(message: unknown) => void>();
  return { values, listeners };
}

function testEnvironment({
  enabled,
  focusWindow = vi.fn(),
  focused,
  openTask = vi.fn(),
  permission,
  shared = sharedBrowserState(),
}: {
  enabled: boolean;
  focusWindow?: () => void;
  focused: boolean;
  openTask?: (taskId: string, title: string) => void;
  permission: NotificationPermission;
  shared?: ReturnType<typeof sharedBrowserState>;
}) {
  let currentFocused = focused;
  let currentPermission = permission;
  let currentTime = Date.parse("2026-07-15T09:30:00.000Z");
  const focusListeners = new Set<() => void>();
  const notifications: TestNotification[] = [];
  const requestPermission = vi.fn(async () => {
    currentPermission = environment.permissionRequestResult;
    return currentPermission;
  });
  if (enabled) {
    shared.values.set("openaide.desktopNotifications.enabled", "true");
    shared.values.set("openaide.desktopNotifications.enabledAt", "2026-07-15T09:00:00.000Z");
  }
  const environment: WebTaskNotificationEnvironment & {
    notifications: TestNotification[];
    permissionRequestResult: NotificationPermission;
    requestPermission: ReturnType<typeof vi.fn>;
    setFocused(focused: boolean): void;
    setNow(timestamp: string): void;
  } = {
    notifications,
    permissionRequestResult: permission,
    requestPermission,
    notificationIconUrl: "/openaide.png",
    storage: {
      getItem: (key) => shared.values.get(key) ?? null,
      setItem: (key, value) => shared.values.set(key, value),
      removeItem: (key) => shared.values.delete(key),
    },
    now: () => currentTime,
    isFocused: () => currentFocused,
    notificationPermission: () => currentPermission,
    requestNotificationPermission: requestPermission,
    notificationsSupported: () => true,
    showNotification(title, options, onClick) {
      const notification: TestNotification = {
        title,
        options,
        closed: false,
        click: onClick,
      };
      notifications.push(notification);
      return { close: () => { notification.closed = true; } };
    },
    focusWindow,
    openTask,
    subscribeFocus(listener) {
      focusListeners.add(listener);
      return () => focusListeners.delete(listener);
    },
    publish(message) {
      for (const listener of shared.listeners) listener(message);
    },
    subscribeMessages(listener) {
      shared.listeners.add(listener);
      return () => shared.listeners.delete(listener);
    },
    setFocused(nextFocused) {
      currentFocused = nextFocused;
      for (const listener of focusListeners) listener();
    },
    setNow(timestamp) {
      currentTime = Date.parse(timestamp);
    },
  };
  return environment;
}
