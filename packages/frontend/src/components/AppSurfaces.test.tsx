import { act, create } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import type { AppController } from "./appController";
import { AppSurfaces } from "./AppSurfaces";
import { createInitialState, type AppState } from "../state/store";

type TestController = AppController & { state: AppState };

const VSCODE_SHELL = { kind: "vscodeExtension", navigationMode: "currentProject" } as const;
const WEB_SHELL = { kind: "web", navigationMode: "project" } as const;

const surfaceMocks = vi.hoisted(() => ({
  newTask: vi.fn(() => null),
  settings: vi.fn(() => null),
  sidebar: vi.fn(() => null),
  task: vi.fn(() => null),
  taskLoading: vi.fn(() => null),
  updateTaskSurfaceTitle: vi.fn(),
}));

function latestMockProps<T>(mock: { mock: { calls: unknown[][] } }) {
  return mock.mock.calls.at(-1)?.[0] as T | undefined;
}

vi.mock("./Sidebar", () => ({
  DEFAULT_MAX_TASKS_PER_PROJECT: 15,
  Sidebar: surfaceMocks.sidebar,
}));

vi.mock("./settings/SettingsView", () => ({
  SettingsView: surfaceMocks.settings,
}));

vi.mock("./TaskView", () => ({
  TaskLoadingView: surfaceMocks.taskLoading,
  TaskView: surfaceMocks.task,
}));

vi.mock("./NewTaskView", () => ({
  NewTaskView: surfaceMocks.newTask,
}));

vi.mock("../services/hostBridge", async (importOriginal) => ({
  ...await importOriginal<typeof import("../services/hostBridge")>(),
  updateTaskSurfaceTitle: surfaceMocks.updateTaskSurfaceTitle,
}));

describe("AppSurfaces callback wiring", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    surfaceMocks.newTask.mockClear();
    surfaceMocks.settings.mockClear();
    surfaceMocks.sidebar.mockClear();
    surfaceMocks.task.mockClear();
    surfaceMocks.taskLoading.mockClear();
    surfaceMocks.updateTaskSurfaceTitle.mockClear();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("passes navigation callbacks to the sidebar", () => {
    const controller = controllerFor("navigation");

    render(controller);

    expect(surfaceMocks.sidebar).toHaveBeenCalledWith(
      expect.objectContaining({
        onArchiveTask: controller.callbacks.navigation.archiveTask,
        onLoadNativeSessions: controller.callbacks.navigation.loadNativeSessions,
        onNewTask: controller.callbacks.navigation.openNewTask,
        onOpenNativeSession: controller.callbacks.navigation.openNativeSession,
        onOpenTask: controller.callbacks.navigation.openTask,
        onRestoreTask: controller.callbacks.navigation.restoreTask,
        onSearchChange: controller.callbacks.navigation.changeSearch,
        onSettings: controller.callbacks.navigation.openSettings,
        onToggleArchived: controller.callbacks.navigation.toggleArchived,
      }),
      undefined,
    );
  });

  it("groups VS Code Task Navigation even for one Project", () => {
    const controller = controllerFor("navigation");
    controller.bootstrap = {
      surface: "navigation",
      shell: { kind: "vscodeExtension", navigationMode: "currentProject" },
      appServerConnection: {
        kind: "webProxy",
        endpointUrl: "/transport-must-not-control-navigation",
      },
    } as AppController["bootstrap"];
    controller.state.projects = [{
      projectId: "project_1",
      label: "OpenAIDE",
    }];

    render(controller);

    expect(surfaceMocks.sidebar).toHaveBeenCalledWith(
      expect.objectContaining({
        groupByProject: true,
        maxTasksPerProject: 15,
        projects: controller.state.projects,
      }),
      undefined,
    );
  });

  it("passes settings callbacks to settings view", () => {
    const controller = controllerFor("settings");

    render(controller);

    expect(surfaceMocks.settings).toHaveBeenCalledWith(
      expect.objectContaining({
        onAuthenticate: expect.any(Function),
        onCreateCustomAgent: controller.callbacks.settings.createCustomAgent,
        onDeleteCustomAgent: controller.callbacks.settings.deleteCustomAgent,
        onRefresh: controller.callbacks.settings.refreshSettings,
        onReplaceCustomAgent: controller.callbacks.settings.replaceCustomAgent,
        onSelectTab: controller.callbacks.settings.selectSettingsTab,
        onSetAcpTrace: controller.callbacks.settings.setAcpTrace,
        onSetAgentEnabled: controller.callbacks.settings.setAgentEnabled,
        onSetComposerSubmitShortcut: controller.callbacks.settings.setComposerSubmitShortcut,
        onUpdateCustomAgentMetadata: controller.callbacks.settings.updateCustomAgentMetadata,
        onUnlockDeveloperSettings: controller.callbacks.settings.unlockDeveloperSettings,
      }),
      undefined,
    );
  });

  it("refreshes Agent Settings after retrying setup", async () => {
    const controller = controllerFor("settings");
    render(controller);
    const lastCall = surfaceMocks.settings.mock.calls.at(-1) as unknown as [{
      recoveryActions: { onRetry: (agentId: string) => Promise<boolean> };
    }] | undefined;

    await act(async () => {
      await lastCall![0].recoveryActions.onRetry("codex");
    });

    expect(controller.callbacks.navigation.retryAgent).toHaveBeenCalledWith("codex");
    expect(controller.callbacks.settings.refreshSettings).toHaveBeenCalledOnce();
  });

  it("returns successful recovery authentication to the preserved New Task", async () => {
    const controller = controllerFor("settings");
    controller.bootstrap = {
      surface: "settings",
      shell: VSCODE_SHELL,
      projectId: "project_1",
      returnToNewTask: true,
      settingsAgentId: "codex",
    };
    vi.mocked(controller.callbacks.settings.authenticateAgent).mockResolvedValue(true);
    render(controller);
    const lastCall = surfaceMocks.settings.mock.calls.at(-1) as unknown as [{
      onAuthenticate: (agentId: string, methodId: string) => Promise<boolean>;
    }] | undefined;
    const props = lastCall?.[0];

    await act(async () => {
      await props!.onAuthenticate("codex", "codex-login");
    });

    expect(controller.callbacks.navigation.openNewTask).toHaveBeenCalledWith("project_1");
  });

  it("renders web settings inside the web workbench sidebar shell", () => {
    const controller = controllerFor("settings");
    controller.bootstrap = {
      surface: "settings",
      shell: WEB_SHELL,
      appServerConnection: {
        kind: "localHttp",
        endpointUrl: "http://127.0.0.1:43123",
        authToken: "test-token",
      },
    };

    render(controller);

    expect(surfaceMocks.sidebar).toHaveBeenCalledWith(
      expect.objectContaining({
        activeTaskId: undefined,
        groupByProject: true,
        onNewTask: expect.any(Function),
        onSettings: expect.any(Function),
        settingsActive: true,
      }),
      undefined,
    );
    expect(surfaceMocks.settings).toHaveBeenCalledWith(
      expect.objectContaining({
        onRefresh: controller.callbacks.settings.refreshSettings,
      }),
      undefined,
    );
  });

  it("keeps the routed Task selected when transient navigation focus is absent", () => {
    const controller = webControllerFor("task");
    controller.bootstrap = {
      surface: "task",
      shell: WEB_SHELL,
      taskId: "task_2",
      appServerConnection: {
        kind: "webProxy",
        endpointUrl: "/__openaide-app-server/probe",
      },
    };
    controller.activeNavigationTaskId = undefined;

    render(controller);

    expect(surfaceMocks.sidebar).toHaveBeenCalledWith(
      expect.objectContaining({ activeTaskId: "task_2" }),
      undefined,
    );
  });

  it("opens mobile web navigation after a left-edge swipe", () => {
    stubMobileWindow();
    const controller = webControllerFor("task");
    const tree = render(controller);
    const shell = tree.root.findByType("main");

    act(() => {
      shell.props.onPointerDownCapture(pointerEvent({ clientX: 8, clientY: 120, pointerId: 1 }));
      shell.props.onPointerMoveCapture(pointerEvent({ clientX: 160, clientY: 126, pointerId: 1 }));
      shell.props.onPointerUp(pointerEvent({ clientX: 160, clientY: 126, pointerId: 1 }));
    });

    expect(tree.root.findByProps({ "aria-label": "Close task navigation" }).props["aria-expanded"]).toBe(true);
  });

  it("keeps the mobile navigation backdrop mounted while closed so exit motion can finish", () => {
    stubMobileWindow();
    const tree = render(webControllerFor("task"));

    const backdrop = tree.root.findByProps({ className: "mobile-navigation-backdrop" });

    expect(backdrop.props["aria-hidden"]).toBe("true");
  });

  it("moves mobile navigation with an in-progress edge swipe", () => {
    stubMobileWindow();
    const tree = render(webControllerFor("task"));
    let shell = tree.root.findByType("main");

    act(() => {
      shell.props.onPointerDownCapture(pointerEvent({ clientX: 8, clientY: 120, pointerId: 1 }));
      shell.props.onPointerMoveCapture(pointerEvent({ clientX: 80, clientY: 124, pointerId: 1 }));
    });

    shell = tree.root.findByType("main");
    expect(shell.props.className).toContain("mobile-navigation-dragging");
    expect(shell.props.style["--mobile-navigation-progress"]).toBeCloseTo(0.25, 2);
    expect(latestMockProps<{ hiddenFromAccessibility?: boolean }>(surfaceMocks.sidebar)?.hiddenFromAccessibility).toBe(false);
  });

  it("shows task status in the narrow workbench header", () => {
    const controller = webControllerFor("task");
    controller.state.snapshot = snapshot("task_1");

    const tree = render(controller);
    const header = tree.root.findByProps({ className: "mobile-workbench-bar" });

    expect(header.findByType("small").children.join("")).toBe("Ready · OpenAIDE");
  });

  it("ignores mobile web navigation swipes that do not start at the left edge", () => {
    stubMobileWindow();
    const controller = webControllerFor("task");
    const tree = render(controller);
    const shell = tree.root.findByType("main");

    act(() => {
      shell.props.onPointerDownCapture(pointerEvent({ clientX: 80, clientY: 120, pointerId: 1 }));
      shell.props.onPointerMoveCapture(pointerEvent({ clientX: 160, clientY: 122, pointerId: 1 }));
      shell.props.onPointerUp(pointerEvent({ clientX: 160, clientY: 122, pointerId: 1 }));
    });

    expect(tree.root.findByProps({ "aria-label": "Open task navigation" }).props["aria-expanded"]).toBe(false);
  });

  it("keeps mobile web navigation closed for vertical edge drags", () => {
    stubMobileWindow();
    const controller = webControllerFor("task");
    const tree = render(controller);
    const shell = tree.root.findByType("main");

    act(() => {
      shell.props.onPointerDownCapture(pointerEvent({ clientX: 8, clientY: 120, pointerId: 1 }));
      shell.props.onPointerMoveCapture(pointerEvent({ clientX: 30, clientY: 190, pointerId: 1 }));
      shell.props.onPointerUp(pointerEvent({ clientX: 30, clientY: 190, pointerId: 1 }));
    });

    expect(tree.root.findByProps({ "aria-label": "Open task navigation" }).props["aria-expanded"]).toBe(false);
  });

  it("does not capture normal taps inside the open mobile web navigation", () => {
    stubMobileWindow();
    const controller = webControllerFor("task");
    const tree = render(controller);
    const shell = tree.root.findByType("main");
    const setPointerCapture = vi.fn();

    act(() => {
      tree.root.findByProps({ "aria-label": "Open task navigation" }).props.onClick();
    });
    act(() => {
      shell.props.onPointerDownCapture(pointerEvent({
        clientX: 120,
        clientY: 800,
        pointerId: 1,
        setPointerCapture,
      }));
      shell.props.onPointerUp(pointerEvent({ clientX: 120, clientY: 800, pointerId: 1 }));
    });

    expect(setPointerCapture).not.toHaveBeenCalled();
    expect(tree.root.findByProps({ "aria-label": "Close task navigation" }).props["aria-expanded"]).toBe(true);
  });

  it("returns an interrupted drawer swipe to its starting state", () => {
    stubMobileWindow();
    const tree = render(webControllerFor("task"));

    act(() => {
      tree.root.findByProps({ "aria-label": "Open task navigation" }).props.onClick();
    });
    const shell = tree.root.findByType("main");
    act(() => {
      shell.props.onPointerDownCapture(pointerEvent({ clientX: 240, clientY: 120, pointerId: 1 }));
      shell.props.onPointerMoveCapture(pointerEvent({ clientX: 50, clientY: 124, pointerId: 1 }));
      shell.props.onPointerCancel(pointerEvent({ clientX: 50, clientY: 124, pointerId: 1 }));
    });

    expect(tree.root.findByProps({ "aria-label": "Close task navigation" }).props["aria-expanded"]).toBe(true);
  });

  it("closes mobile navigation when a drawer swipe crosses the settle distance", () => {
    stubMobileWindow();
    const tree = render(webControllerFor("task"));

    act(() => {
      tree.root.findByProps({ "aria-label": "Open task navigation" }).props.onClick();
    });
    const shell = tree.root.findByType("main");
    act(() => {
      shell.props.onPointerDownCapture(pointerEvent({ clientX: 240, clientY: 120, pointerId: 1 }));
      shell.props.onPointerMoveCapture(pointerEvent({ clientX: 50, clientY: 124, pointerId: 1 }));
      shell.props.onPointerUp(pointerEvent({ clientX: 50, clientY: 124, pointerId: 1 }));
    });

    expect(tree.root.findByProps({ "aria-label": "Open task navigation" }).props["aria-expanded"]).toBe(false);
  });

  it("opens mobile navigation with a short fast edge fling", () => {
    stubMobileWindow();
    const tree = render(webControllerFor("task"));
    const shell = tree.root.findByType("main");

    act(() => {
      shell.props.onPointerDownCapture(pointerEvent({ clientX: 8, clientY: 120, pointerId: 1, timeStamp: 0 }));
      shell.props.onPointerMoveCapture(pointerEvent({ clientX: 80, clientY: 124, pointerId: 1, timeStamp: 60 }));
      shell.props.onPointerUp(pointerEvent({ clientX: 80, clientY: 124, pointerId: 1, timeStamp: 70 }));
    });

    expect(tree.root.findByProps({ "aria-label": "Close task navigation" }).props["aria-expanded"]).toBe(true);
  });

  it("does not treat an old edge movement as a fling after the finger pauses", () => {
    stubMobileWindow();
    const tree = render(webControllerFor("task"));
    const shell = tree.root.findByType("main");

    act(() => {
      shell.props.onPointerDownCapture(pointerEvent({ clientX: 8, clientY: 120, pointerId: 1, timeStamp: 0 }));
      shell.props.onPointerMoveCapture(pointerEvent({ clientX: 80, clientY: 124, pointerId: 1, timeStamp: 50 }));
      shell.props.onPointerUp(pointerEvent({ clientX: 80, clientY: 124, pointerId: 1, timeStamp: 200 }));
    });

    expect(tree.root.findByProps({ "aria-label": "Open task navigation" }).props["aria-expanded"]).toBe(false);
  });

  it("hides the web main surface from assistive tech while mobile navigation is open", () => {
    stubMobileWindow();
    const controller = webControllerFor("settings");
    const tree = render(controller);

    expect(tree.root.findByProps({ className: "web-main-surface" }).props["aria-hidden"]).toBeUndefined();
    expect(tree.root.findByProps({ className: "web-main-surface" }).props.inert).toBeUndefined();

    act(() => {
      tree.root.findByProps({ "aria-label": "Open task navigation" }).props.onClick();
    });

    expect(tree.root.findByProps({ className: "web-main-surface" }).props["aria-hidden"]).toBe(true);
    expect(tree.root.findByProps({ className: "web-main-surface" }).props.inert).toBe(true);
  });

  it("hides the closed mobile web navigation from assistive tech", () => {
    stubMobileWindow();
    const controller = webControllerFor("settings");
    const tree = render(controller);

    expect(latestMockProps<{ hiddenFromAccessibility?: boolean }>(surfaceMocks.sidebar)?.hiddenFromAccessibility).toBe(true);

    act(() => {
      tree.root.findByProps({ "aria-label": "Open task navigation" }).props.onClick();
    });

    expect(latestMockProps<{ hiddenFromAccessibility?: boolean }>(surfaceMocks.sidebar)?.hiddenFromAccessibility).toBe(false);
    expect(latestMockProps<{ modal?: boolean }>(surfaceMocks.sidebar)?.modal).toBe(true);
  });

  it("keeps desktop web navigation available to assistive tech", () => {
    vi.stubGlobal("window", {
      addEventListener: vi.fn(),
      innerWidth: 1200,
      matchMedia: vi.fn(() => ({ matches: false })),
      removeEventListener: vi.fn(),
      requestAnimationFrame: (callback: FrameRequestCallback) => callback(0),
    });
    const controller = webControllerFor("settings");

    render(controller);

    expect(latestMockProps<{ hiddenFromAccessibility?: boolean }>(surfaceMocks.sidebar)?.hiddenFromAccessibility).toBe(false);
  });

  it("releases the main surface when an open mobile drawer crosses into desktop layout", () => {
    let mobile = true;
    let onViewportChange: (() => void) | undefined;
    vi.stubGlobal("window", {
      addEventListener: vi.fn(),
      innerWidth: 390,
      matchMedia: vi.fn(() => ({
        get matches() { return mobile; },
        addEventListener: (_type: string, listener: () => void) => { onViewportChange = listener; },
        removeEventListener: vi.fn(),
      })),
      removeEventListener: vi.fn(),
    });
    const tree = render(webControllerFor("task"));

    act(() => {
      tree.root.findByProps({ "aria-label": "Open task navigation" }).props.onClick();
    });
    expect(tree.root.findByProps({ className: "web-main-surface" }).props.inert).toBe(true);

    act(() => {
      mobile = false;
      onViewportChange?.();
    });

    expect(tree.root.findByProps({ className: "web-main-surface" }).props.inert).toBeUndefined();
    expect(latestMockProps<{ modal?: boolean }>(surfaceMocks.sidebar)?.modal).toBe(false);
  });

  it("passes active task callbacks to task view", () => {
    const controller = controllerFor("task");
    controller.state.snapshot = snapshot("task_1", true);

    render(controller);

    expect(surfaceMocks.task).toHaveBeenCalledWith(
      expect.objectContaining({
        onCancel: controller.callbacks.task.cancel,
        onLoadChatPage: controller.callbacks.task.loadChatPage,
        onSubscribeToolDetail: controller.callbacks.task.subscribeToolDetail,
        onPermissionRespond: controller.callbacks.task.respondToPermission,
        onRevealAttachment: controller.callbacks.task.revealAttachment,
        onRemoveAttachment: controller.callbacks.task.removeAttachment,
        onSendPrompt: controller.callbacks.task.sendPrompt,
        backendReady: false,
      }),
      undefined,
    );
  });

  it("keeps cached task history visible with the in-place refresh retry", () => {
    const controller = controllerFor("task");
    controller.bootstrap = { surface: "task", shell: VSCODE_SHELL, taskId: "task_1" };
    controller.state.snapshot = snapshot("task_1", true);
    // A concurrent subscription snapshot can clear taskOpenError after task/open fails.
    controller.backendConnectionState = { status: "unavailable", message: "Connection closed." };

    render(controller);

    expect(surfaceMocks.task).toHaveBeenCalledWith(
      expect.objectContaining({
        backendConnectionState: controller.backendConnectionState,
        onRetryConnection: controller.retryTaskOpen,
        snapshot: controller.state.snapshot,
      }),
      undefined,
    );
    expect(surfaceMocks.taskLoading).not.toHaveBeenCalled();
  });

  it("keeps the New Task surface visible while authoritative Send is pending", () => {
    const controller = controllerFor("task");
    controller.state.snapshot = snapshot("task_starting", false);
    controller.state.newTask.submitting = true;
    controller.state.taskInputs.task_starting = {
      prompt: "",
      context: [],
      pending: { prompt: "Build the thing", context: [], state: "sending" },
    };

    render(controller);

    expect(surfaceMocks.newTask).toHaveBeenCalledWith(
      expect.objectContaining({ onCancelTask: controller.callbacks.newTask.cancel }),
      undefined,
    );
    expect(surfaceMocks.task).not.toHaveBeenCalled();
  });

  it("passes archive context and restore action to the task view", () => {
    const controller = controllerFor("task");
    controller.state.snapshot = snapshot("task_1", true);
    controller.state.showArchived = true;
    controller.activeTask = controller.state.snapshot.task;

    render(controller);

    expect(surfaceMocks.task).toHaveBeenCalledWith(
      expect.objectContaining({
        archived: true,
        onRestoreTask: controller.callbacks.navigation.restoreTask,
      }),
      undefined,
    );
  });

  it("does not mark the open task archived just because the sidebar shows archive", () => {
    const controller = controllerFor("task");
    controller.state.snapshot = snapshot("task_1", true);
    controller.state.showArchived = true;

    render(controller);

    expect(surfaceMocks.task).toHaveBeenCalledWith(
      expect.objectContaining({
        archived: false,
      }),
      undefined,
    );
  });

  it("renders empty task snapshots through the new task view", () => {
    const controller = controllerFor("task");
    controller.state.snapshot = snapshot("task_1", false);

    render(controller);

    expect(surfaceMocks.newTask).toHaveBeenCalledWith(
      expect.objectContaining({
        onSubmitTask: controller.callbacks.newTask.submit,
      }),
      undefined,
    );
    expect(surfaceMocks.task).not.toHaveBeenCalled();
  });

  it("keeps an active prepared New Task on the new-task surface", () => {
    const controller = webControllerFor("task");
    controller.state.snapshot = snapshot("task_prepared", false);
    controller.state.snapshot.task.status = "active";
    controller.activeTask = controller.state.snapshot.task;

    const tree = render(controller);

    expect(surfaceMocks.newTask).toHaveBeenCalledWith(
      expect.objectContaining({
        onSubmitTask: controller.callbacks.newTask.submit,
      }),
      undefined,
    );
    expect(surfaceMocks.task).not.toHaveBeenCalled();
    expect(tree.root.findByProps({ className: "mobile-workbench-bar" }).findByType("small").children.join(""))
      .toBe("OpenAIDE");
  });

  it("renders pending empty task snapshots through the task view", () => {
    const controller = controllerFor("task");
    controller.bootstrap = { surface: "task", shell: VSCODE_SHELL, taskId: "task_1" };
    controller.state.snapshot = snapshot("task_1", false);
    controller.state.taskInputs.task_1 = {
      prompt: "",
      context: [],
      pending: { prompt: "Build the thing", context: [], state: "sending" },
    };
    controller.state.newTask.pending = {
      prompt: "Build the thing",
      context: [],
      configOptions: {
        agent_id: "codex",
        options: [{
          category: "model",
          kind: "select", current_value: { type: "id", value: "gpt-5.5" },
          id: "model",
          label: "Model",
          values: [{ id: "gpt-5.5", label: "GPT-5.5" }],
        }],
        status: "ready",
      },
    };

    render(controller);

    expect(surfaceMocks.task).toHaveBeenCalledWith(
      expect.objectContaining({
        snapshot: controller.state.snapshot,
        startupConfigOptions: controller.state.newTask.pending.configOptions,
        taskInput: controller.state.taskInputs.task_1,
      }),
      undefined,
    );
    expect(surfaceMocks.updateTaskSurfaceTitle).toHaveBeenCalledWith(
      "task_1",
      controller.state.snapshot.task.title,
    );
    expect(surfaceMocks.newTask).not.toHaveBeenCalled();
  });

  it("renders an active no-message Task with its Task-scoped Stop action", () => {
    const controller = controllerFor("task");
    controller.bootstrap = { surface: "task", shell: VSCODE_SHELL, taskId: "task_1" };
    controller.state.snapshot = snapshot("task_1", false);
    controller.state.snapshot.task.status = "active";

    render(controller);

    expect(surfaceMocks.task).toHaveBeenCalledWith(
      expect.objectContaining({
        onCancel: controller.callbacks.task.cancel,
        snapshot: controller.state.snapshot,
      }),
      undefined,
    );
    expect(surfaceMocks.taskLoading).not.toHaveBeenCalled();
  });

  it("keeps a failed first-send draft visible on its adopted Task route", () => {
    const controller = controllerFor("task");
    controller.bootstrap = { surface: "task", shell: VSCODE_SHELL, taskId: "task_1" };
    controller.state.snapshot = snapshot("task_1", false);
    controller.state.taskInputs.task_1 = {
      prompt: "Build the thing",
      context: [],
      error: "Connection closed before Send was acknowledged.",
    };

    render(controller);

    expect(surfaceMocks.task).toHaveBeenCalledWith(
      expect.objectContaining({
        snapshot: controller.state.snapshot,
        taskInput: controller.state.taskInputs.task_1,
      }),
      undefined,
    );
    expect(surfaceMocks.taskLoading).not.toHaveBeenCalled();
  });

  it("keeps an authoritatively rejected first-send draft editable on its Task route", () => {
    const controller = controllerFor("task");
    controller.bootstrap = { surface: "task", shell: VSCODE_SHELL, taskId: "task_1" };
    controller.state.snapshot = snapshot("task_1", false);
    controller.state.taskInputs.task_1 = {
      prompt: "Inspect this file",
      context: [{
        kind: "file",
        label: "notes.md",
        local_id: "attachment-1",
        validation_error: "Reselect this file.",
      }],
      error: "Attachment is no longer available.",
    };

    render(controller);

    expect(surfaceMocks.task).toHaveBeenCalledWith(
      expect.objectContaining({
        snapshot: controller.state.snapshot,
        taskInput: controller.state.taskInputs.task_1,
      }),
      undefined,
    );
    expect(surfaceMocks.taskLoading).not.toHaveBeenCalled();
  });

  it("renders web task loading state while an existing task snapshot is opening", () => {
    const controller = webControllerFor("task");
    controller.bootstrap = {
      surface: "task",
      shell: WEB_SHELL,
      taskId: "task_1",
      appServerConnection: {
        kind: "webProxy",
        endpointUrl: "/__openaide-app-server/probe",
      },
    };

    render(controller);

    expect(surfaceMocks.taskLoading).toHaveBeenCalledWith(
      expect.objectContaining({
        error: undefined,
      }),
      undefined,
    );
    expect(surfaceMocks.newTask).not.toHaveBeenCalled();
  });

  it("offers the in-place retry after task opening fails", () => {
    const controller = controllerFor("task");
    controller.bootstrap = { surface: "task", shell: VSCODE_SHELL, taskId: "task_1" };
    controller.state.taskOpenError = { taskId: "task_1", message: "Connection closed." };

    render(controller);

    expect(surfaceMocks.taskLoading).toHaveBeenCalledWith(
      expect.objectContaining({
        error: "Connection closed.",
        onRetry: controller.retryTaskOpen,
      }),
      undefined,
    );
  });

  it("renders task loading state while a native session is opening", () => {
    const controller = webControllerFor("nativeSession");
    controller.bootstrap = {
      surface: "nativeSession",
      shell: WEB_SHELL,
      agentId: "codex",
      nativeSessionId: "native_1",
    };
    controller.state.newTask.nativeSessions.adoptingSessionId = "native_1";

    render(controller);

    expect(surfaceMocks.taskLoading).toHaveBeenCalledWith(
      expect.objectContaining({
        error: undefined,
        label: "Opening session",
      }),
      undefined,
    );
    expect(surfaceMocks.newTask).not.toHaveBeenCalled();
  });

  it("replaces the previous task chat with loading while a native session is opening", () => {
    const controller = webControllerFor("nativeSession");
    controller.bootstrap = {
      surface: "nativeSession",
      shell: WEB_SHELL,
      agentId: "codex",
      nativeSessionId: "native_1",
    };
    controller.state.activeTaskId = "task_previous";
    controller.state.snapshot = snapshot("task_previous");
    controller.state.newTask.nativeSessions.adoptingSessionId = "native_1";

    render(controller);

    expect(surfaceMocks.taskLoading).toHaveBeenCalledWith(
      expect.objectContaining({ error: undefined, label: "Opening session" }),
      undefined,
    );
    expect(surfaceMocks.task).not.toHaveBeenCalled();
  });

  it("keeps the Native Session route visible when adoption reports not-found", () => {
    const controller = webControllerFor("nativeSession");
    controller.bootstrap = {
      surface: "nativeSession",
      shell: WEB_SHELL,
      agentId: "codex",
      nativeSessionId: "native_1",
    };
    controller.state.newTask.nativeSessions.adoptionError = {
      sessionId: "native_1",
      message: "This session no longer exists.",
    };

    render(controller);

    expect(surfaceMocks.taskLoading).toHaveBeenCalledWith(
      expect.objectContaining({
        error: "This session no longer exists.",
        label: "Opening session",
        onRetry: undefined,
      }),
      undefined,
    );
  });

  it("passes new-task callbacks to new task view", () => {
    const controller = controllerFor("task");

    render(controller);

    expect(surfaceMocks.newTask).toHaveBeenCalledWith(
      expect.objectContaining({
        onSelectConfigOption: controller.callbacks.newTask.selectConfigOption,
        onSubmitTask: controller.callbacks.newTask.submit,
      }),
      undefined,
    );
  });

  it("requests new-task composer focus when New task is invoked in the web workbench", () => {
    vi.stubGlobal("window", { requestAnimationFrame: (callback: FrameRequestCallback) => callback(0) });
    const controller = webControllerFor("task");
    const tree = render(controller);
    const firstNewTaskProps = latestMockProps<{ focusRequestKey: number }>(surfaceMocks.newTask);
    const sidebarProps = latestMockProps<{ onNewTask: () => void }>(surfaceMocks.sidebar);
    expect(firstNewTaskProps).toBeDefined();
    expect(sidebarProps).toBeDefined();
    const firstFocusKey = firstNewTaskProps?.focusRequestKey ?? 0;
    const onNewTask = sidebarProps?.onNewTask;
    expect(onNewTask).toBeDefined();

    act(() => {
      onNewTask?.();
    });

    const secondFocusKey = latestMockProps<{ focusRequestKey: number }>(surfaceMocks.newTask)?.focusRequestKey;
    expect(secondFocusKey).toBe(firstFocusKey + 1);
    expect(controller.callbacks.navigation.openNewTask).toHaveBeenCalled();
    expect(tree.root.findByProps({ "aria-label": "Open task navigation" }).props["aria-expanded"]).toBe(false);
  });

  it("passes project loading state to new task view until backend initialize completes", () => {
    const controller = controllerFor("task");

    render(controller);

    expect(surfaceMocks.newTask).toHaveBeenCalledWith(
      expect.objectContaining({
        loadingProjects: true,
      }),
      undefined,
    );

    surfaceMocks.newTask.mockClear();
    controller.backendReady = true;
    render(controller);

    expect(surfaceMocks.newTask).toHaveBeenCalledWith(
      expect.objectContaining({
        loadingProjects: false,
      }),
      undefined,
    );
  });

  it("passes task loading state to the web workbench sidebar until backend initialize completes", () => {
    const controller = webControllerFor("task");
    controller.backendReady = false;
    controller.visibleTasks = [];
    controller.state.projects = [];

    render(controller);

    expect(surfaceMocks.sidebar).toHaveBeenCalledWith(
      expect.objectContaining({
        loadingTasks: true,
      }),
      undefined,
    );

    surfaceMocks.sidebar.mockClear();
    controller.backendReady = true;
    render(controller);

    expect(surfaceMocks.sidebar).toHaveBeenCalledWith(
      expect.objectContaining({
        loadingTasks: false,
      }),
      undefined,
    );
  });

  it("shows a web App Server error instead of empty loading state", () => {
    const controller = controllerFor("task");
    controller.bootstrap = {
      surface: "task",
      shell: WEB_SHELL,
      appServerConnection: {
        kind: "webProxy",
        endpointUrl: "/__openaide-app-server/probe",
      },
    };
    controller.state.appServerError = "App Server request timed out.";

    const tree = render(controller).toJSON();

    expect(JSON.stringify(tree)).toContain("App Server connection unavailable.");
    expect(JSON.stringify(tree)).toContain("App Server request timed out.");
    expect(surfaceMocks.newTask).not.toHaveBeenCalled();
  });

  it("shows an editor App Server initialization failure instead of an endless connecting composer", () => {
    const controller = controllerFor("task");
    controller.state.appServerError = "App Server request timed out.";

    const rendered = JSON.stringify(render(controller).toJSON());

    expect(rendered).toContain("App Server connection unavailable.");
    expect(rendered).toContain("App Server request timed out.");
    expect(rendered).not.toContain("Retry");
    expect(surfaceMocks.newTask).not.toHaveBeenCalled();
  });
});

function render(controller: TestController) {
  controller.view = viewFor(controller.state);
  let tree!: ReturnType<typeof create>;
  act(() => {
    tree = create(<AppSurfaces controller={controller} />);
  });
  return tree;
}

function controllerFor(surface: AppController["bootstrap"]["surface"]): TestController {
  const state = createInitialState();
  return {
    activeTask: undefined,
    agents: [],
    backendConnectionState: { status: "connecting" },
    backendReady: false,
    bootstrap: surface === "invalid" ? { surface } : { surface, shell: VSCODE_SHELL },
    callbacks: {
      navigation: {
        archiveTask: vi.fn(),
        changeSearch: vi.fn(),
        loadNativeSessions: vi.fn(),
        openNativeSession: vi.fn(),
        openNewTask: vi.fn(),
        openSettings: vi.fn(),
        retryAgent: vi.fn(async () => true),
        openTask: vi.fn(),
        restoreTask: vi.fn(),
        setTaskTitle: vi.fn(),
        toggleArchived: vi.fn(),
      },
      newTask: {
        cancel: vi.fn(),
        removeAttachment: vi.fn(),
        selectConfigOption: vi.fn(),
        submit: vi.fn(),
      },
      settings: {
        authenticateAgent: vi.fn(),
        createCustomAgent: vi.fn(),
        deleteCustomAgent: vi.fn(),
        replaceCustomAgent: vi.fn(),
        refreshSettings: vi.fn(),
        selectSettingsTab: vi.fn(),
        setAcpTrace: vi.fn(),
        setAgentEnabled: vi.fn(),
        setComposerSubmitShortcut: vi.fn(),
        updateCustomAgentMetadata: vi.fn(),
        unlockDeveloperSettings: vi.fn(),
      },
      task: {
        cancel: vi.fn(),
        loadChatPage: vi.fn(),
        subscribeToolDetail: vi.fn(() => vi.fn()),
        revealAttachment: vi.fn(),
        removeAttachment: vi.fn(),
        respondToPermission: vi.fn(),
        respondToQuestion: vi.fn(),
        selectConfigOption: vi.fn(),
        sendPrompt: vi.fn(),
      },
    },
    intents: {
      newTask: {
        changePrompt: vi.fn(),
        reportAttachmentError: vi.fn(),
        selectAgent: vi.fn(),
        selectIsolation: vi.fn(),
        selectProject: vi.fn(),
        selectWorkspace: vi.fn(),
        selectWorktree: vi.fn(),
        refreshWorktrees: vi.fn(),
        createWorktree: vi.fn(),
        recreateWorktree: vi.fn(),
        removeWorktree: vi.fn(),
        removalPreflight: vi.fn(),
        renameWorktree: vi.fn(),
        openFolder: vi.fn(),
        openTask: vi.fn(),
      },
      task: {
        changePrompt: vi.fn(),
        recordScroll: vi.fn(),
        refreshWorkspace: vi.fn(),
        reportAttachmentError: vi.fn(),
      },
    },
    preferences: { composer_submit_shortcut: "mod_enter" },
    retryTaskOpen: vi.fn(),
    state,
    view: viewFor(state),
    visibleTasks: [],
  };
}

function webControllerFor(surface: "nativeSession" | "settings" | "task"): TestController {
  const controller = controllerFor(surface);
  controller.bootstrap = {
    surface,
    shell: WEB_SHELL,
    appServerConnection: {
      kind: "webProxy",
      endpointUrl: "/__openaide-app-server/probe",
    },
  };
  return controller;
}

function viewFor(state: AppState): AppController["view"] {
  const taskId = state.snapshot?.task.task_id;
  return {
    appServerError: state.appServerError,
    navigation: {
      nativeSessions: state.newTask.nativeSessions,
      newTaskSelection: state.newTask.selection,
      projects: state.projects,
      searchQuery: state.searchQuery,
      showArchived: state.showArchived,
      taskListError: state.taskListError,
    },
    primaryTask: {
      chatPageState: taskId ? state.chatPages[taskId] : undefined,
      liveTextPresentation: taskId ? state.taskLiveTextPresentation[taskId] : undefined,
      newTask: {
        newTask: state.newTask,
        preparedTaskInput: taskId ? state.taskInputs[taskId] : undefined,
        projects: state.projects,
        tasks: state.tasks,
        worktreeRepositories: state.worktreeRepositories,
        snapshot: state.snapshot,
        workspaceRootsLoaded: state.workspaceRootsLoaded,
      },
      permissionResponses: state.permissionResponses,
      questionResponses: state.questionResponses,
      savedScrollState: taskId ? state.taskChatScrollStates[taskId] : undefined,
      snapshot: state.snapshot,
      taskInput: taskId ? state.taskInputs[taskId] : undefined,
      taskOpenError: state.taskOpenError,
      toolDetails: state.toolDetails,
    },
    settings: state.settings,
  };
}

function pointerEvent({
  clientX,
  clientY,
  pointerId,
  setPointerCapture = vi.fn(),
  releasePointerCapture = vi.fn(),
  timeStamp = 0,
}: {
  clientX: number;
  clientY: number;
  pointerId: number;
  setPointerCapture?: (pointerId: number) => void;
  releasePointerCapture?: (pointerId: number) => void;
  timeStamp?: number;
}) {
  return {
    buttons: 1,
    clientX,
    clientY,
    currentTarget: {
      releasePointerCapture,
      setPointerCapture,
    },
    pointerId,
    pointerType: "touch",
    timeStamp,
  };
}

function stubMobileWindow() {
  vi.stubGlobal("window", {
    addEventListener: vi.fn(),
    innerWidth: 390,
    matchMedia: vi.fn(() => ({ matches: true })),
    removeEventListener: vi.fn(),
  });
}

function snapshot(taskId: string, hasMessages = true): TaskSnapshot {
  return {
    lifecycle: hasMessages ? "open" : "prepared",
    chat: {
      has_before: false,
      has_messages: hasMessages,
      items: [],
      task_id: taskId,
      total_count: hasMessages ? 1 : 0,
      version: 1,
    },
    active_requests: [],
    history_sync: { state: "idle", generation: 0 },
    send_capability: { state: "ready" },
    revision: 1,
    settings_summary: {
      agent_id: "codex",
      isolation: "local",
    },
    task: {
      agent_id: "codex",
      agent_name: "Codex",
      created_at: "2026-05-22T00:00:00.000Z",
      isolation: "local",
      last_activity: "2026-05-22T00:00:00.000Z",
      message_history_version: 1,
      has_messages: hasMessages,
      status: "inactive",
      task_id: taskId,
      task_version: 1,
      title: "Task",
      unread: false,
      updated_at: "2026-05-22T00:00:00.000Z",
      workspace_root: "/workspace",
    },
  };
}
