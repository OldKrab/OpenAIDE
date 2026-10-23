import { describe, expect, it, vi } from "vitest";
import type { ActionPerformed, Options } from "@tauri-apps/plugin-notification";
import type { TaskSummary } from "@openaide/app-shell-contracts";
import { createDesktopTaskNotifications, type DesktopNotificationHost } from "./desktopTaskNotifications";

describe("Desktop Task notifications", () => {
  it("is on by default and obtains permission without a prompt", async () => {
    const host = fakeHost();
    const manager = createDesktopTaskNotifications(host);
    await settled();

    expect(manager.getSettings()).toEqual({ status: "enabled" });
  });

  it("stays off across restarts once the user turns it off", async () => {
    const host = fakeHost();
    await createDesktopTaskNotifications(host).setEnabled(false);

    const restarted = createDesktopTaskNotifications(host);
    await settled();

    expect(restarted.getSettings()).toEqual({ status: "off" });
  });

  it("stays silent for attention that existed before notifications turned on by default", async () => {
    const host = fakeHost();
    const manager = createDesktopTaskNotifications(host);
    await settled();
    manager.reconcile("root-1", []);

    manager.reconcile("root-1", [task("attention-early", "2026-10-16T08:59:00.000Z")]);

    expect(host.sent).toHaveLength(0);
  });

  it("shows an unattended Task Attention Event natively and routes its click to the Task", async () => {
    const host = fakeHost();
    const manager = createDesktopTaskNotifications(host);
    await settled();
    await manager.setEnabled(true);
    manager.reconcile("root-1", []);

    manager.reconcile("root-1", [task("attention-1")]);

    expect(host.sent).toEqual([expect.objectContaining({
      title: "Answer needed",
      body: "Fix the build",
      extra: { taskId: "task-1" },
    })]);
    host.tap(host.sent[0]);
    await settled();
    expect(host.openTask).toHaveBeenCalledWith("task-1");
    expect(host.focusWindow).toHaveBeenCalledTimes(1);
  });

  it("stays quiet for an event of the Task the focused window shows", async () => {
    const host = fakeHost({ focused: true, focusedTaskId: "task-1" });
    const manager = createDesktopTaskNotifications(host);
    await settled();
    await manager.setEnabled(true);
    manager.reconcile("root-1", []);

    manager.reconcile("root-1", [task("attention-1")]);

    expect(host.sent).toHaveLength(0);
  });

  it("notifies while the focused window shows another Task", async () => {
    const host = fakeHost({ focused: true, focusedTaskId: "task-2" });
    const manager = createDesktopTaskNotifications(host);
    await settled();
    await manager.setEnabled(true);
    manager.reconcile("root-1", []);

    manager.reconcile("root-1", [task("attention-1")]);

    expect(host.sent).toHaveLength(1);
  });

  it("records each decision in the Desktop log without the Task title", async () => {
    const host = fakeHost({ focused: true, focusedTaskId: "task-1" });
    vi.spyOn(console, "info").mockImplementation(() => undefined);
    const manager = createDesktopTaskNotifications(host);
    await settled();
    manager.reconcile("root-1", [task("attention-backlog")]);

    manager.reconcile("root-1", [task("attention-seen"), task("attention-unseen", undefined, "task-2")]);
    manager.reconcile("root-1", [task("attention-seen")]);

    expect(host.recorded).toEqual([
      "desktop_notification_permission outcome=granted",
      "task_attention_notification outcome=baseline count=1",
      "task_attention_notification outcome=suppressed event_id=attention-seen reason=needsAnswer cause=attended",
      "desktop_notification_shown notification_id=1",
      "task_attention_notification outcome=shown event_id=attention-unseen reason=needsAnswer",
      "task_attention_notification outcome=withdrawn count=1 cause=superseded",
    ]);
    expect(host.recorded.join("\n")).not.toContain("Fix the build");
    vi.restoreAllMocks();
  });

  it("routes a click on a notification shown before the window reloaded", async () => {
    const host = fakeHost();
    createDesktopTaskNotifications(host);
    await settled();

    host.tap({ id: 41, title: "Task finished", extra: { taskId: "task-9" } });
    await settled();

    expect(host.openTask).toHaveBeenCalledWith("task-9");
    expect(host.focusWindow).toHaveBeenCalledTimes(1);
  });
});

function fakeHost({ focused = false, focusedTaskId }: { focused?: boolean; focusedTaskId?: string } = {}) {
  const values = new Map<string, string>();
  let actionListener: ((event: ActionPerformed) => void) | undefined;
  const now = Date.parse("2026-10-16T09:00:00.000Z");
  const sent: Options[] = [];
  const openTask = vi.fn<(taskId: string) => void>();
  const focusWindow = vi.fn<() => Promise<void>>(async () => undefined);
  const recorded: string[] = [];
  const host: DesktopNotificationHost & {
    sent: Options[];
    recorded: string[];
    openTask: typeof openTask;
    focusWindow: typeof focusWindow;
    tap(options: Options): void;
  } = {
    sent,
    recorded,
    storage: {
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => { values.set(key, value); },
      removeItem: (key) => { values.delete(key); },
    },
    isPermissionGranted: async () => false,
    requestPermission: async () => "granted",
    sendNotification: (options) => { sent.push(options); },
    removeActive: async () => undefined,
    onAction: async (listener) => { actionListener = listener; },
    initiallyFocused: () => focused,
    isFocused: async () => focused,
    onFocusChanged: async () => undefined,
    focusedTaskId: () => focusedTaskId,
    focusWindow,
    openTask,
    now: () => now,
    record: (event, fields) => {
      recorded.push([event, ...Object.entries(fields).map(([key, value]) => `${key}=${value}`)].join(" "));
    },
    tap(options) {
      actionListener?.({
        actionId: "tap",
        inputValue: null,
        notification: { ...options, id: options.id ?? 0, groupSummary: false, data: {}, extra: options.extra ?? {}, attachments: [] },
      });
    },
  };
  return host;
}

function task(eventId: string, occurredAt = "2026-10-16T09:00:30.000Z", taskId = "task-1"): TaskSummary {
  return {
    task_id: taskId,
    title: "Fix the build",
    attention: { event_id: eventId, reason: "needsAnswer", occurred_at: occurredAt },
  } as TaskSummary;
}

async function settled() {
  for (let index = 0; index < 5; index += 1) await Promise.resolve();
}
