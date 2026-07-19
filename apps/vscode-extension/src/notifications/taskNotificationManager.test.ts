import { describe, expect, it, vi } from "vitest";
import type { TaskSummary } from "@openaide/app-server-client";
import {
  createTaskNotificationManager,
  type TaskNotificationEnvironment,
} from "./taskNotificationManager";

describe("VS Code Task notifications", () => {
  it("suppresses the startup backlog and shows a new unattended event", async () => {
    const test = environment(false, Date.parse("2026-07-20T12:00:00.000Z"));
    const manager = createTaskNotificationManager(test.environment);

    manager.reconcile([task("old-event", "finished", "2026-07-20T11:59:00.000Z")]);
    manager.reconcile([task("new-event", "needsAnswer", "2026-07-20T12:00:01.000Z")]);

    expect(test.notifications).toEqual([{
      message: "Task needs an answer: Review the implementation",
      action: "Open Task",
    }]);

    test.resolveNotification("Open Task");
    await Promise.resolve();
    expect(test.openTask).toHaveBeenCalledWith("task-1", "Review the implementation");
  });

  it("does not notify later for an event that occurred while VS Code was focused", () => {
    const occurredAt = Date.parse("2026-07-20T12:00:01.000Z");
    const test = environment(true, Date.parse("2026-07-20T12:00:00.000Z"));
    const manager = createTaskNotificationManager(test.environment);

    manager.reconcile([]);
    test.setNow(Date.parse("2026-07-20T12:00:02.000Z"));
    test.setFocused(false);
    manager.reconcile([task("focused-event", "finished", new Date(occurredAt).toISOString())]);

    expect(test.notifications).toEqual([]);
  });

  it("notifies when another Task was focused at the event occurrence time", () => {
    const test = environment(
      true,
      Date.parse("2026-07-20T12:00:00.000Z"),
      "task-2",
    );
    const manager = createTaskNotificationManager(test.environment);

    manager.reconcile([]);
    manager.reconcile([task("other-task-event", "finished", "2026-07-20T12:00:01.000Z")]);

    expect(test.notifications[0]?.message).toBe("Task finished: Review the implementation");
  });

  it("accepts App Server epoch-millisecond attention timestamps", () => {
    const test = environment(false, 1_784_505_208_000);
    const manager = createTaskNotificationManager(test.environment);

    manager.reconcile([]);
    manager.reconcile([task("epoch-event", "finished", "1784505209618")]);

    expect(test.notifications[0]?.message).toBe("Task finished: Review the implementation");
  });

  it("delivers an event that occurred while unfocused even if VS Code refocused first", () => {
    const test = environment(true, Date.parse("2026-07-20T12:00:00.000Z"));
    const manager = createTaskNotificationManager(test.environment);

    manager.reconcile([]);
    test.setNow(Date.parse("2026-07-20T12:00:01.000Z"));
    test.setFocused(false);
    test.setNow(Date.parse("2026-07-20T12:00:03.000Z"));
    test.setFocused(true);
    manager.reconcile([task("unfocused-event", "failed", "2026-07-20T12:00:02.000Z")]);

    expect(test.notifications[0]?.message).toBe("Task failed: Review the implementation");
  });

  it("persists delivery receipts so another manager does not duplicate an event", () => {
    const test = environment(false, Date.parse("2026-07-20T12:00:00.000Z"));
    const first = createTaskNotificationManager(test.environment);
    first.reconcile([]);
    first.reconcile([task("handled-event", "stopped", "2026-07-20T12:00:01.000Z")]);
    first.dispose();

    const second = createTaskNotificationManager(test.environment);
    second.reconcile([]);
    second.reconcile([task("handled-event", "stopped", "2026-07-20T12:00:01.000Z")]);

    expect(test.notifications).toHaveLength(1);
  });
});

function task(
  eventId: string,
  reason: NonNullable<TaskSummary["attention"]>["reason"],
  occurredAt: string,
): TaskSummary {
  return {
    taskId: "task-1" as TaskSummary["taskId"],
    projectId: "project-1" as TaskSummary["projectId"],
    agentId: "codex" as TaskSummary["agentId"],
    title: { value: "Review the implementation" },
    status: "idle",
    updatedAt: occurredAt,
    lastActivity: occurredAt,
    unread: true,
    attention: { eventId, reason, occurredAt },
    hasMessages: true,
  };
}

function environment(
  initiallyFocused: boolean,
  initialNow: number,
  initialFocusedTaskId: string | undefined = "task-1",
) {
  let focused = initiallyFocused;
  let focusedTaskId = initialFocusedTaskId;
  let now = initialNow;
  let resolveNotification: (selection: string | undefined) => void = () => undefined;
  const focusListeners = new Set<(focused: boolean) => void>();
  const focusedTaskListeners = new Set<(taskId: string | undefined) => void>();
  const handled = new Set<string>();
  const notifications: Array<{ message: string; action: string }> = [];
  const openTask = vi.fn();
  const environment: TaskNotificationEnvironment = {
    now: () => now,
    isFocused: () => focused,
    focusedTaskId: () => focusedTaskId,
    readHandledEventIds: () => [...handled],
    rememberHandledEventIds: (eventIds) => {
      handled.clear();
      for (const eventId of eventIds) handled.add(eventId);
    },
    showNotification(message, action) {
      notifications.push({ message, action });
      return new Promise((resolve) => {
        resolveNotification = resolve;
      });
    },
    openTask,
    subscribeFocus(listener) {
      focusListeners.add(listener);
      return () => focusListeners.delete(listener);
    },
    subscribeFocusedTask(listener) {
      focusedTaskListeners.add(listener);
      return () => focusedTaskListeners.delete(listener);
    },
  };
  return {
    environment,
    notifications,
    openTask,
    resolveNotification: (selection: string | undefined) => resolveNotification(selection),
    setFocused(next: boolean) {
      focused = next;
      for (const listener of focusListeners) listener(focused);
    },
    setNow(next: number) {
      now = next;
    },
    setFocusedTask(taskId: string | undefined) {
      focusedTaskId = taskId;
      for (const listener of focusedTaskListeners) listener(focusedTaskId);
    },
  };
}
