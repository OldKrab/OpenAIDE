import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppServerStateObserver, SubscriptionScope } from "@openaide/app-server-client";
import { registerTaskNotifications } from "./taskNotifications";

const vscodeMocks = vi.hoisted(() => ({
  showInformationMessage: vi.fn(async () => "Open Task"),
  windowState: { focused: true },
  windowStateListeners: new Set<(state: { focused: boolean }) => void>(),
  executeCommand: vi.fn(async () => undefined),
}));

vi.mock("vscode", () => ({
  window: {
    state: vscodeMocks.windowState,
    onDidChangeWindowState: vi.fn((listener) => {
      vscodeMocks.windowStateListeners.add(listener);
      return { dispose: () => vscodeMocks.windowStateListeners.delete(listener) };
    }),
    showInformationMessage: vscodeMocks.showInformationMessage,
  },
  commands: {
    executeCommand: vscodeMocks.executeCommand,
  },
}));

vi.mock("../workspace/roots", () => ({
  workspaceRoots: () => [{ projectId: "project-1" }],
}));

describe("VS Code Task notification registration", () => {
  beforeEach(() => {
    vscodeMocks.windowState.focused = true;
    vscodeMocks.windowStateListeners.clear();
    vscodeMocks.showInformationMessage.mockClear();
    vscodeMocks.executeCommand.mockClear();
  });

  it("subscribes once at extension-host scope and routes the notification action", async () => {
    let observer: AppServerStateObserver | undefined;
    const stop = vi.fn();
    const runtime = {
      subscribeAppServerState: vi.fn(async (
        scope: SubscriptionScope,
        nextObserver: AppServerStateObserver,
      ) => {
        observer = nextObserver;
        return stop;
      }),
    };
    const values = new Map<string, unknown>();
    const globalState = {
      get: vi.fn((key: string, fallback: unknown) => values.get(key) ?? fallback),
      update: vi.fn(async (key: string, value: unknown) => {
        values.set(key, value);
      }),
    };
    const openTask = vi.fn();
    const disposeTaskFocus = vi.fn();
    const logger = { warn: vi.fn(), info: vi.fn() };

    const registration = await registerTaskNotifications(
      runtime,
      globalState,
      {
        openTask,
        currentFocusedTaskId: () => "task-2",
        onDidChangeFocusedTask: vi.fn(() => ({ dispose: disposeTaskFocus })),
      },
      logger,
    );
    expect(runtime.subscribeAppServerState).toHaveBeenCalledWith(
      {
        kind: "taskNavigation",
        section: "tasks",
        projectIds: ["project-1"],
      },
      expect.any(Object),
    );

    observer?.onSnapshot(navigationSnapshot([]));
    observer?.onSnapshot(navigationSnapshot([{
      taskId: "task-1",
      projectId: "project-1",
      agentId: "codex",
      title: { value: "Ship notifications" },
      status: "idle",
      updatedAt: "2026-07-20T12:00:01.000Z",
      lastActivity: "2026-07-20T12:00:01.000Z",
      unread: true,
      attention: {
        eventId: "event-1",
        reason: "finished",
        occurredAt: "2026-07-20T12:00:01.000Z",
      },
      hasMessages: true,
    }]));
    await vi.waitFor(() => expect(openTask).toHaveBeenCalledWith("task-1", "Ship notifications"));

    registration.dispose();
    expect(stop).toHaveBeenCalledOnce();
    expect(disposeTaskFocus).toHaveBeenCalledOnce();
    expect(vscodeMocks.windowStateListeners).toHaveLength(0);
  });

  it("uses an OS notification while the VS Code window is unfocused", async () => {
    vscodeMocks.windowState.focused = false;
    let observer: AppServerStateObserver | undefined;
    const runtime = {
      subscribeAppServerState: vi.fn(async (
        _scope: SubscriptionScope,
        nextObserver: AppServerStateObserver,
      ) => {
        observer = nextObserver;
        return vi.fn();
      }),
    };
    const globalState = {
      get: vi.fn((_key: string, fallback: unknown) => fallback),
      update: vi.fn(async () => undefined),
    };

    const openTask = vi.fn();
    await registerTaskNotifications(
      runtime,
      globalState,
      {
        openTask,
        currentFocusedTaskId: () => "task-1",
        onDidChangeFocusedTask: vi.fn(() => ({ dispose: vi.fn() })),
      },
      { warn: vi.fn(), info: vi.fn() },
    );

    observer?.onSnapshot(navigationSnapshot([]));
    observer?.onSnapshot(navigationSnapshot([attentionTask()]));
    await vi.waitFor(() => expect(vscodeMocks.executeCommand).toHaveBeenCalledWith(
      "_openaide.notifications.show",
      { message: "Task finished: Ship notifications" },
    ));
    expect(vscodeMocks.showInformationMessage).not.toHaveBeenCalled();
  });

  it("falls back to the workbench notification when OS delivery is unavailable", async () => {
    vscodeMocks.windowState.focused = false;
    vscodeMocks.executeCommand.mockRejectedValueOnce(new Error("local presenter unavailable"));
    let observer: AppServerStateObserver | undefined;
    const runtime = {
      subscribeAppServerState: vi.fn(async (
        _scope: SubscriptionScope,
        nextObserver: AppServerStateObserver,
      ) => {
        observer = nextObserver;
        return vi.fn();
      }),
    };
    const globalState = {
      get: vi.fn((_key: string, fallback: unknown) => fallback),
      update: vi.fn(async () => undefined),
    };

    const openTask = vi.fn();
    await registerTaskNotifications(
      runtime,
      globalState,
      {
        openTask,
        currentFocusedTaskId: () => "task-1",
        onDidChangeFocusedTask: vi.fn(() => ({ dispose: vi.fn() })),
      },
      { warn: vi.fn(), info: vi.fn() },
    );

    observer?.onSnapshot(navigationSnapshot([]));
    observer?.onSnapshot(navigationSnapshot([attentionTask()]));

    await vi.waitFor(() => expect(vscodeMocks.showInformationMessage).toHaveBeenCalledWith(
      "Task finished: Ship notifications",
      "Open Task",
    ));
    await vi.waitFor(() => expect(openTask).toHaveBeenCalledWith("task-1", "Ship notifications"));
  });
});

function attentionTask(): import("@openaide/app-server-client").TaskSummary {
  const occurredAt = new Date(Date.now() + 1_000).toISOString();
  return {
    taskId: "task-1" as import("@openaide/app-server-client").TaskId,
    projectId: "project-1" as import("@openaide/app-server-client").ProjectId,
    agentId: "codex" as import("@openaide/app-server-client").AgentId,
    title: { value: "Ship notifications" },
    status: "idle",
    updatedAt: occurredAt,
    lastActivity: occurredAt,
    unread: true,
    attention: {
      eventId: "event-1",
      reason: "finished",
      occurredAt,
    },
    hasMessages: true,
  };
}

function navigationSnapshot(
  tasks: import("@openaide/app-server-client").TaskSummary[],
): import("@openaide/app-server-client").SubscriptionSnapshot {
  return {
    kind: "taskNavigation",
    navigation: {
      section: "tasks",
      groups: [{
        projectId: "project-1" as import("@openaide/app-server-client").ProjectId,
        projectLabel: "Project",
        taskCount: tasks.length,
        entries: tasks.map((task) => ({ kind: "task" as const, task })),
      }],
      refresh: { state: "idle" },
    },
  };
}
