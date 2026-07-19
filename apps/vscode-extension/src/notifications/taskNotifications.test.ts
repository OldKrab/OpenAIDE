import { describe, expect, it, vi } from "vitest";
import type { AppServerStateObserver, SubscriptionScope } from "@openaide/app-server-client";
import { registerTaskNotifications } from "./taskNotifications";

const vscodeMocks = vi.hoisted(() => ({
  focused: false,
  focusListener: undefined as ((event: { focused: boolean }) => void) | undefined,
  showInformationMessage: vi.fn(async () => "Open Task"),
  disposeFocus: vi.fn(),
}));

vi.mock("vscode", () => ({
  window: {
    state: {
      get focused() {
        return vscodeMocks.focused;
      },
    },
    onDidChangeWindowState: vi.fn((listener) => {
      vscodeMocks.focusListener = listener;
      return { dispose: vscodeMocks.disposeFocus };
    }),
    showInformationMessage: vscodeMocks.showInformationMessage,
  },
}));

describe("VS Code Task notification registration", () => {
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
      { kind: "taskNavigation" },
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
    expect(vscodeMocks.disposeFocus).toHaveBeenCalledOnce();
    expect(disposeTaskFocus).toHaveBeenCalledOnce();
  });
});

function navigationSnapshot(
  tasks: import("@openaide/app-server-client").TaskSummary[],
): import("@openaide/app-server-client").SubscriptionSnapshot {
  return {
    kind: "taskNavigation",
    navigation: { tasks },
  };
}
