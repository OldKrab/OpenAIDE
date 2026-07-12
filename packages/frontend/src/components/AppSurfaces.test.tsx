import { act, create } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import type { AppController } from "./appController";
import { AppSurfaces } from "./AppSurfaces";
import { createInitialState } from "../state/store";

const surfaceMocks = vi.hoisted(() => ({
  newTask: vi.fn(() => null),
  settings: vi.fn(() => null),
  sidebar: vi.fn(() => null),
  task: vi.fn(() => null),
  taskLoading: vi.fn(() => null),
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

describe("AppSurfaces callback wiring", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    surfaceMocks.newTask.mockClear();
    surfaceMocks.settings.mockClear();
    surfaceMocks.sidebar.mockClear();
    surfaceMocks.task.mockClear();
    surfaceMocks.taskLoading.mockClear();
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

  it("passes settings callbacks to settings view", () => {
    const controller = controllerFor("settings");

    render(controller);

    expect(surfaceMocks.settings).toHaveBeenCalledWith(
      expect.objectContaining({
        onAuthenticate: controller.callbacks.settings.authenticateAgent,
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

  it("renders web settings inside the web workbench sidebar shell", () => {
    const controller = controllerFor("settings");
    controller.bootstrap = {
      surface: "settings",
      appServerConnection: {
        kind: "webProxy",
        endpointUrl: "/__openaide-app-server/probe",
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
        onLoadToolDetail: controller.callbacks.task.loadToolDetail,
        onPermissionRespond: controller.callbacks.task.respondToPermission,
        onRevealAttachment: controller.callbacks.task.revealAttachment,
        onRemoveAttachment: controller.callbacks.task.removeAttachment,
        onSendPrompt: controller.callbacks.task.sendPrompt,
        backendReady: false,
      }),
      undefined,
    );
  });

  it("keeps the New Task surface visible while authoritative Send is pending", () => {
    const controller = controllerFor("task");
    controller.state.snapshot = snapshot("task_starting", false);
    controller.state.newTask.submitting = true;
    controller.state.taskInputs.task_starting = {
      prompt: "",
      context: [],
      pending: { prompt: "Build the thing", context: [] },
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

  it("renders pending empty task snapshots through the task view", () => {
    const controller = controllerFor("task");
    controller.bootstrap = { surface: "task", taskId: "task_1" };
    controller.state.snapshot = snapshot("task_1", false);
    controller.state.taskInputs.task_1 = {
      prompt: "",
      context: [],
      pending: { prompt: "Build the thing", context: [] },
    };
    controller.state.newTask.pending = {
      prompt: "Build the thing",
      context: [],
      configOptions: {
        agent_id: "codex",
        options: [{
          category: "model",
          current_value: "gpt-5.5",
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
    expect(surfaceMocks.newTask).not.toHaveBeenCalled();
  });

  it("renders web task loading state while an existing task snapshot is opening", () => {
    const controller = webControllerFor("task");
    controller.bootstrap = {
      surface: "task",
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

  it("renders task loading state while a native session is opening", () => {
    const controller = webControllerFor("task");
    controller.state.newTask.nativeSessions.adoptingSessionId = "native_1";

    render(controller);

    expect(surfaceMocks.taskLoading).toHaveBeenCalledWith(
      expect.objectContaining({
        error: undefined,
      }),
      undefined,
    );
    expect(surfaceMocks.newTask).not.toHaveBeenCalled();
  });

  it("replaces the previous task chat with loading while a native session is opening", () => {
    const controller = webControllerFor("task");
    controller.state.activeTaskId = "task_previous";
    controller.state.snapshot = snapshot("task_previous");
    controller.state.newTask.nativeSessions.adoptingSessionId = "native_1";

    render(controller);

    expect(surfaceMocks.taskLoading).toHaveBeenCalledWith(
      expect.objectContaining({ error: undefined }),
      undefined,
    );
    expect(surfaceMocks.task).not.toHaveBeenCalled();
  });

  it("passes new-task callbacks to new task view", () => {
    const controller = controllerFor("task");

    render(controller);

    expect(surfaceMocks.newTask).toHaveBeenCalledWith(
      expect.objectContaining({
        onSelectConfigOption: controller.callbacks.newTask.selectConfigOption,
        onSubmitTask: controller.callbacks.newTask.submit,
        resetOptionsRequestKey: controller.callbacks.newTask.resetOptionsRequestKey,
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
});

function render(controller: AppController) {
  let tree!: ReturnType<typeof create>;
  act(() => {
    tree = create(<AppSurfaces controller={controller} />);
  });
  return tree;
}

function controllerFor(surface: AppController["bootstrap"]["surface"]): AppController {
  const state = createInitialState();
  return {
    activeTask: undefined,
    agents: [],
    backendReady: false,
    bootstrap: { surface },
    callbacks: {
      navigation: {
        archiveTask: vi.fn(),
        changeSearch: vi.fn(),
        loadNativeSessions: vi.fn(),
        openNativeSession: vi.fn(),
        openNewTask: vi.fn(),
        openSettings: vi.fn(),
        openTask: vi.fn(),
        restoreTask: vi.fn(),
        toggleArchived: vi.fn(),
      },
      newTask: {
        cancel: vi.fn(),
        resetOptionsRequestKey: vi.fn(),
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
        loadToolDetail: vi.fn(),
        revealAttachment: vi.fn(),
        removeAttachment: vi.fn(),
        respondToPermission: vi.fn(),
        respondToQuestion: vi.fn(),
        retryHistory: vi.fn(),
        selectConfigOption: vi.fn(),
        sendPrompt: vi.fn(),
      },
    },
    createSnapshotRequestId: vi.fn(),
    dispatch: vi.fn(),
    preferences: { composer_submit_shortcut: "mod_enter" },
    state,
    visibleTasks: [],
  };
}

function webControllerFor(surface: "settings" | "task"): AppController {
  const controller = controllerFor(surface);
  controller.bootstrap = {
    surface,
    appServerConnection: {
      kind: "webProxy",
      endpointUrl: "/__openaide-app-server/probe",
    },
  };
  return controller;
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
    chat: {
      has_before: false,
      has_messages: hasMessages,
      items: [],
      task_id: taskId,
      total_count: hasMessages ? 1 : 0,
      version: 1,
    },
    permissions: [],
    history_sync: { state: "idle", generation: 0 },
    send_capability: { state: "ready", attachment_only: true },
    revision: 1,
    settings_summary: {
      agent_id: "codex",
      config_options: {},
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
