import { act, create } from "react-test-renderer";
import type { ComponentProps } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import type { AppController } from "./appController";
import { AppSurfaces } from "./AppSurfaces";
import { createInitialState, type AppState } from "../state/store";
import type { FrontendShell } from "../services/frontendShell";

type TestController = AppController & { state: AppState };

const VSCODE_SHELL = { kind: "vscodeExtension", navigationMode: "currentProject" } as const;
const WEB_SHELL = { kind: "web", navigationMode: "project" } as const;
const DESKTOP_SHELL = { kind: "desktop", navigationMode: "project" } as const;

const surfaceMocks = vi.hoisted(() => ({
  newTask: vi.fn(() => null),
  renderRealSidebar: false,
  settings: vi.fn(() => null),
  sidebar: vi.fn((_props?: unknown, _context?: unknown) => null),
  task: vi.fn(() => null),
  taskLoading: vi.fn(() => null),
  updateTaskSurfaceTitle: vi.fn(),
}));

const frontendShellMocks = vi.hoisted(() => ({
  current: undefined as Partial<FrontendShell> | undefined,
}));

function latestMockProps<T>(mock: { mock: { calls: unknown[][] } }) {
  return mock.mock.calls.at(-1)?.[0] as T | undefined;
}

vi.mock("./Sidebar", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./Sidebar")>();
  const { createElement } = await import("react");
  return {
    ...actual,
    Sidebar: (props: React.ComponentProps<typeof actual.Sidebar>, context: unknown) => {
      surfaceMocks.sidebar(props, context);
      return surfaceMocks.renderRealSidebar ? createElement(actual.Sidebar, props) : null;
    },
  };
});

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

vi.mock("../services/frontendShell", async (importOriginal) => ({
  ...await importOriginal<typeof import("../services/frontendShell")>(),
  currentFrontendShell: () => frontendShellMocks.current as FrontendShell | undefined,
}));

describe("AppSurfaces callback wiring", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    surfaceMocks.newTask.mockClear();
    surfaceMocks.renderRealSidebar = false;
    surfaceMocks.settings.mockClear();
    surfaceMocks.sidebar.mockClear();
    surfaceMocks.task.mockClear();
    surfaceMocks.taskLoading.mockClear();
    surfaceMocks.updateTaskSurfaceTitle.mockClear();
    frontendShellMocks.current = undefined;
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
        onNewTask: expect.any(Function),
        onOpenNativeSession: controller.callbacks.navigation.openNativeSession,
        onOpenTask: controller.callbacks.navigation.openTask,
        onRestoreTask: controller.callbacks.navigation.restoreTask,
        onSearchChange: controller.callbacks.navigation.changeSearch,
        onSettings: expect.any(Function),
        onToggleArchived: controller.callbacks.navigation.toggleArchived,
      }),
      undefined,
    );
  });

  it("opens the global VS Code New Task action without overriding the shell-retained Project", () => {
    const controller = controllerFor("navigation");
    controller.state.newTask.selection.projectId = "project-codearts";

    render(controller);
    const sidebarProps = latestMockProps<{ onNewTask: (projectId?: string) => void }>(surfaceMocks.sidebar);

    act(() => sidebarProps?.onNewTask());

    expect(controller.callbacks.navigation.openNewTask).toHaveBeenCalledWith(undefined);
  });

  it("keeps a Project-scoped VS Code New Task action explicit", () => {
    const controller = controllerFor("navigation");
    controller.state.newTask.selection.projectId = "project-codearts";

    render(controller);
    const sidebarProps = latestMockProps<{ onNewTask: (projectId?: string) => void }>(surfaceMocks.sidebar);

    act(() => sidebarProps?.onNewTask("project-agent-kernel"));

    expect(controller.callbacks.navigation.openNewTask).toHaveBeenCalledWith("project-agent-kernel");
  });

  it("passes shell-provided workspace recovery to Task Navigation", () => {
    const controller = controllerFor("navigation");
    const openFolder = vi.fn();
    controller.workspaceSetup = { openFolder };

    render(controller);

    expect(surfaceMocks.sidebar).toHaveBeenCalledWith(
      expect.objectContaining({ onOpenWorkspaceFolder: openFolder }),
      undefined,
    );
  });

  it("passes shell-provided workspace recovery to New Task", () => {
    const controller = controllerFor("task");
    const openFolder = vi.fn();
    controller.workspaceSetup = { openFolder };

    render(controller);

    expect(surfaceMocks.newTask).toHaveBeenCalledWith(
      expect.objectContaining({ onOpenWorkspaceFolder: openFolder }),
      undefined,
    );
  });

  it("opens the Web Project folder dialog from the sidebar footer", async () => {
    const controller = controllerFor("navigation");
    controller.bootstrap = { surface: "navigation", shell: WEB_SHELL };
    controller.callbacks.newTask.workspaceBrowser = {
      ownerKey: "project-browser",
      listDirectory: vi.fn(async () => ({
        directory: { label: "Computer", path: "/", parentPath: null },
        entries: [{ label: "home", path: "/home" }],
      })),
      listRoots: vi.fn(async () => [{ label: "Computer", path: "/" }]),
    };
    const tree = render(controller);
    const sidebar = latestMockProps<ComponentProps<typeof import("./Sidebar").Sidebar>>(surfaceMocks.sidebar);

    await act(async () => sidebar?.onAddProject?.());

    expect(tree.root.findByProps({ "aria-label": "Add Project" })).toBeDefined();
    expect(controller.callbacks.newTask.workspaceBrowser.listRoots).toHaveBeenCalledOnce();
  });

  it("uses the native Project folder picker on Desktop", async () => {
    const controller = controllerFor("navigation");
    controller.bootstrap = { surface: "navigation", shell: DESKTOP_SHELL };
    const listRoots = vi.fn(async () => [{ label: "Computer", path: "/" }]);
    controller.callbacks.newTask.workspaceBrowser = {
      ownerKey: "project-browser",
      listDirectory: vi.fn(),
      listRoots,
    };
    const pickFolder = vi.fn(async () => ({ path: "C:\\workspace\\OpenAIDE", label: "OpenAIDE" }));
    frontendShellMocks.current = { projects: { pickFolder } };
    controller.intents.projects.add = vi.fn(async () => ({
      projectId: "project_1" as never,
      label: "OpenAIDE",
      workspaceRoot: "C:\\workspace\\OpenAIDE",
      available: true,
    }));
    const tree = render(controller);
    const sidebar = latestMockProps<ComponentProps<typeof import("./Sidebar").Sidebar>>(surfaceMocks.sidebar);

    await act(async () => sidebar?.onAddProject?.());

    expect(pickFolder).toHaveBeenCalledOnce();
    expect(listRoots).not.toHaveBeenCalled();
    expect(controller.intents.projects.add).toHaveBeenCalledWith("C:\\workspace\\OpenAIDE");
    expect(tree.root.findAllByProps({ "aria-label": "Add Project" })).toHaveLength(0);
  });

  it("delegates Project folder acquisition to the VS Code host", async () => {
    const controller = controllerFor("navigation");
    const listRoots = vi.fn(async () => [{ label: "Computer", path: "/" }]);
    controller.callbacks.newTask.workspaceBrowser = {
      ownerKey: "project-browser",
      listDirectory: vi.fn(),
      listRoots,
    };
    const openFolder = vi.fn();
    frontendShellMocks.current = { workspace: { openFolder } };
    const tree = render(controller);
    const sidebar = latestMockProps<ComponentProps<typeof import("./Sidebar").Sidebar>>(surfaceMocks.sidebar);

    await act(async () => sidebar?.onAddProject?.());

    expect(openFolder).toHaveBeenCalledOnce();
    expect(listRoots).not.toHaveBeenCalled();
    expect(tree.root.findAllByProps({ "aria-label": "Add Project" })).toHaveLength(0);
  });

  it("overlays empty Windows window controls on New Task", () => {
    const controller = controllerFor("task");
    controller.bootstrap = { surface: "task", shell: DESKTOP_SHELL };
    frontendShellMocks.current = {
      desktopWindow: {
        platform: "windows",
        close: vi.fn(async () => undefined),
        minimize: vi.fn(async () => undefined),
        startDragging: vi.fn(async () => undefined),
        toggleMaximize: vi.fn(async () => undefined),
      },
    };

    const tree = render(controller);
    const frame = tree.root.find((node) => typeof node.props.className === "string"
      && node.props.className.split(" ").includes("app-sidebar-frame"));

    expect(frame.props.className).toContain("app-sidebar-frame-with-overlay-header");
    expect(tree.root.findByProps({ "aria-label": "Window controls" })).toBeDefined();
  });

  it("uses the task header as the Windows title bar instead of adding a product-title row", () => {
    const controller = controllerFor("task");
    controller.bootstrap = { surface: "task", shell: DESKTOP_SHELL, taskId: "task_1" };
    controller.state.snapshot = snapshot("task_1");
    const desktopWindow = {
      platform: "windows" as const,
      close: vi.fn(async () => undefined),
      minimize: vi.fn(async () => undefined),
      startDragging: vi.fn(async () => undefined),
      toggleMaximize: vi.fn(async () => undefined),
    };
    frontendShellMocks.current = { desktopWindow };

    const tree = render(controller);
    const frame = tree.root.find((node) => typeof node.props.className === "string"
      && node.props.className.split(" ").includes("app-sidebar-frame"));

    expect(frame.props.className).toContain("app-sidebar-frame-with-overlay-header");
    expect(frame.props.className).toContain("desktop-task-title-bar");
    expect(tree.root.findAllByProps({ className: "desktop-title-bar-label" })).toHaveLength(0);
    expect(latestMockProps<{ desktopWindow?: unknown }>(surfaceMocks.task)?.desktopWindow).toBe(desktopWindow);
  });

  it("limits VS Code New Task Project choices to opened workspace Projects", () => {
    const controller = controllerFor("task");
    controller.bootstrap = {
      surface: "task",
      shell: VSCODE_SHELL,
      projectIds: ["project_1"],
    };
    controller.state.projects = [
      { projectId: "project_1", label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE" },
      { projectId: "project_2", label: "Other", workspaceRoot: "/workspace/Other" },
    ];

    render(controller);

    expect(latestMockProps<{ state: { projects: Array<{ projectId: string }> } }>(surfaceMocks.newTask)
      ?.state.projects)
      .toEqual([{ projectId: "project_1", label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE" }]);
  });

  it("opens Project Worktree Management in the Worktrees Settings tab", () => {
    surfaceMocks.renderRealSidebar = true;
    const controller = controllerFor("navigation");
    showWorktreeProject(controller, "task_2");
    const tree = render(controller);

    act(() => tree.root.findByProps({ "aria-label": "OpenAIDE actions" }).props.onClick());
    act(() => tree.root.findAllByType("button")
      .find((button) => button.children.includes("Manage worktrees"))?.props.onClick());

    expect(controller.callbacks.navigation.openSettings)
      .toHaveBeenCalledWith(undefined, undefined, "project_1", "worktrees");
    expect(tree.root.findAllByProps({ className: "worktree-management" })).toHaveLength(0);
  });

  it("uses the sidebar Project count in removal confirmation", async () => {
    surfaceMocks.renderRealSidebar = true;
    const controller = controllerFor("navigation");
    controller.state.projects = [{
      projectId: "project_1",
      label: "OpenAIDE",
      workspaceRoot: "/workspace/OpenAIDE",
    }];
    controller.state.newTask.nativeSessions.items = Array.from({ length: 6 }, (_, index) => ({
      agent_id: "codex",
      agent_name: "Codex",
      cwd: "/workspace/OpenAIDE",
      project_id: "project_1",
      session_id: `session_${index}`,
      title: `Session ${index}`,
    }));
    controller.intents.newTask.loadProjectTasks = vi.fn(async () => []);
    const tree = render(controller);

    act(() => tree.root.findByProps({ "aria-label": "OpenAIDE actions" }).props.onClick());
    await act(async () => {
      tree.root.findAllByType("button")
        .find((button) => button.children.includes("Remove Project"))?.props.onClick();
      await Promise.resolve();
    });

    const dialog = tree.root.findByProps({ "aria-label": "Remove OpenAIDE" });
    expect(dialog.findByType("p").children.join(""))
      .toBe("6 Tasks will be removed from OpenAIDE. They will remain in their original Agents.");
  });

  it("opens New Task in the adjacent Project after removing the routed Task's Project", async () => {
    surfaceMocks.renderRealSidebar = true;
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
    controller.state.projects = [
      { projectId: "project_1", label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE" },
      { projectId: "project_2", label: "Next", workspaceRoot: "/workspace/Next" },
    ];
    const routedTask = snapshot("task_1").task;
    routedTask.project_id = "project_1";
    routedTask.project_label = "OpenAIDE";
    controller.visibleTasks = [routedTask];
    controller.intents.newTask.loadProjectTasks = vi.fn(async () => [routedTask]);
    controller.intents.projects.remove = vi.fn(async () => 1);
    const tree = render(controller);

    act(() => tree.root.findByProps({ "aria-label": "OpenAIDE actions" }).props.onClick());
    await act(async () => {
      tree.root.findAllByType("button")
        .find((button) => button.children.includes("Remove Project"))?.props.onClick();
      await Promise.resolve();
    });
    const dialog = tree.root.findByProps({ "aria-label": "Remove OpenAIDE" });
    await act(async () => {
      await dialog.findAllByType("button")
        .find((button) => button.children.includes("Remove Project"))?.props.onClick();
    });

    expect(controller.intents.projects.remove).toHaveBeenCalledWith("project_1");
    expect(controller.intents.newTask.selectProject).toHaveBeenCalledWith(controller.state.projects[1]);
    expect(controller.callbacks.navigation.openNewTask).toHaveBeenCalledWith("project_2");
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
        projects: controller.state.projects,
      }),
      undefined,
    );
  });

  it("uses the adaptive Task Navigation row budget for two Projects", () => {
    surfaceMocks.renderRealSidebar = true;
    const controller = controllerFor("navigation");
    controller.state.projects = [
      { projectId: "project_1", label: "OpenAIDE" },
      { projectId: "project_2", label: "Other" },
    ];
    controller.visibleTasks = controller.state.projects.flatMap((project) =>
      Array.from({ length: 24 }, (_, index) => {
        const task = snapshot(`${project.projectId}_task_${index}`).task;
        task.project_id = project.projectId;
        task.project_label = project.label;
        return task;
      }),
    );

    const tree = render(controller);
    const visibleRows = (projectLabel: string) =>
      tree.root.findByProps({ "aria-label": projectLabel }).findAll((node) =>
        node.props.role === "listitem"
        && typeof node.props.className === "string"
        && node.props.className.includes("task-row"),
      );

    expect(visibleRows("OpenAIDE")).toHaveLength(7);
    expect(visibleRows("Other")).toHaveLength(7);
  });

  it("limits current-Project Task Navigation to Projects in the workspace", () => {
    const controller = controllerFor("navigation");
    const currentProject = { projectId: "project_current", label: "ai-bench-runner" };
    const staleProject = { projectId: "project_stale", label: "agent-kernel-workspace" };
    controller.bootstrap = {
      surface: "navigation",
      shell: VSCODE_SHELL,
      projectIds: [currentProject.projectId],
    };
    controller.state.projects = [currentProject, staleProject];

    render(controller);

    expect(latestMockProps<React.ComponentProps<typeof import("./Sidebar").Sidebar>>(
      surfaceMocks.sidebar,
    )?.projects).toEqual([currentProject]);
  });

  it("keeps every Project represented by a multi-root workspace", () => {
    const controller = controllerFor("navigation");
    const firstProject = { projectId: "project_first", label: "api" };
    const secondProject = { projectId: "project_second", label: "web" };
    controller.bootstrap = {
      surface: "navigation",
      shell: VSCODE_SHELL,
      projectIds: [firstProject.projectId, secondProject.projectId],
    };
    controller.state.projects = [
      secondProject,
      { projectId: "project_stale", label: "old-workspace" },
      firstProject,
    ];

    render(controller);

    expect(latestMockProps<React.ComponentProps<typeof import("./Sidebar").Sidebar>>(
      surfaceMocks.sidebar,
    )?.projects).toEqual([firstProject, secondProject]);
  });

  it("hides global Project groups when the current workspace has no folders", () => {
    const controller = controllerFor("navigation");
    controller.bootstrap = {
      surface: "navigation",
      shell: VSCODE_SHELL,
      projectIds: [],
    };
    controller.state.projects = [
      { projectId: "project_stale", label: "agent-kernel-workspace" },
    ];
    controller.workspaceSetup = { openFolder: vi.fn() };

    render(controller);

    const sidebarProps = latestMockProps<React.ComponentProps<typeof import("./Sidebar").Sidebar>>(
      surfaceMocks.sidebar,
    );
    expect(sidebarProps?.projects).toEqual([]);
    expect(sidebarProps?.onOpenWorkspaceFolder).toBe(controller.workspaceSetup.openFolder);
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
        worktreeIntents: controller.intents.newTask,
        worktreeRepositories: controller.state.worktreeRepositories,
      }),
      undefined,
    );
  });

  it("returns web Settings to the previously active Task", () => {
    const controller = controllerFor("settings");
    controller.bootstrap = {
      surface: "settings",
      shell: WEB_SHELL,
      appServerConnection: {
        kind: "webProxy",
        endpointUrl: "/__openaide-app-server/probe",
      },
    };
    controller.activeNavigationTaskId = "task_2";
    render(controller);
    const settingsProps = latestMockProps<{ onBackToApp: () => void }>(surfaceMocks.settings);

    act(() => settingsProps?.onBackToApp());

    expect(controller.callbacks.navigation.openTask).toHaveBeenCalledWith("task_2");
    expect(controller.callbacks.navigation.openNewTask).not.toHaveBeenCalled();
  });

  it("opens New Task with the worktree selected from Settings", () => {
    const controller = controllerFor("settings");
    const project = {
      label: "OpenAIDE",
      projectId: "project_1",
      workspaceRoot: "/workspace/OpenAIDE",
      worktreeRepositoryId: "repository_1",
    };
    render(controller);
    const settingsProps = latestMockProps<{
      onNewTaskInWorktree: (
        selectedProject: typeof project,
        worktree: { name: string; path: string; worktreeId: string },
      ) => void;
    }>(surfaceMocks.settings);

    act(() => settingsProps?.onNewTaskInWorktree(project, {
      name: "Settings refactor",
      path: "/workspace/.worktrees/settings-refactor",
      worktreeId: "worktree_1",
    }));

    expect(controller.intents.newTask.selectProject).toHaveBeenCalledWith(project);
    expect(controller.intents.newTask.selectWorktree).toHaveBeenCalledWith({
      label: "Settings refactor",
      path: "/workspace/.worktrees/settings-refactor",
      worktreeId: "worktree_1",
    });
    expect(controller.callbacks.navigation.openNewTask).toHaveBeenCalledWith("project_1");
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

  it("replaces Task navigation with Settings in the web app", () => {
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

    expect(surfaceMocks.sidebar).not.toHaveBeenCalled();
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
    controller.bootstrap = {
      surface: "task",
      shell: WEB_SHELL,
      taskId: "task_1",
      appServerConnection: {
        kind: "webProxy",
        endpointUrl: "/__openaide-app-server/probe",
      },
    };
    controller.state.snapshot = snapshot("task_1");

    const tree = render(controller);
    const header = tree.root.findByProps({ className: "mobile-workbench-bar" });

    expect(header.findByType("small").children.join("")).toBe("Ready · OpenAIDE");
    expect(header.findByProps({ "aria-label": "Permission handling: Ask every time" })).toBeDefined();
  });

  it("opens the Task Plan from the narrow workbench header", () => {
    stubMobileWindow();
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
    controller.state.snapshot = snapshot("task_1");
    controller.state.snapshot.current_plan = {
      entries: [{ content: "Verify layout", priority: "medium", status: "in_progress" }],
    };

    const tree = render(controller);
    act(() => tree.root.findByProps({ "aria-label": "Open Plan" }).props.onClick());

    expect(tree.root.findByProps({ "aria-label": "Hide Plan" }).props["aria-expanded"]).toBe(true);
    expect(latestMockProps<{ planDrawerOpen?: boolean }>(surfaceMocks.task)?.planDrawerOpen).toBe(true);
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
    const controller = webControllerFor("task");
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
    const controller = webControllerFor("task");
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
    const controller = webControllerFor("task");

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
    controller.bootstrap = { surface: "task", shell: VSCODE_SHELL, taskId: "task_1" };
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

  it("allows project selection for a VS Code New Task with multiple workspace Projects", () => {
    const controller = controllerFor("task");
    controller.bootstrap = {
      surface: "task",
      shell: VSCODE_SHELL,
      projectId: "project_1",
      projectIds: ["project_1", "project_2"],
    };

    render(controller);

    expect(surfaceMocks.newTask).toHaveBeenCalledWith(
      expect.objectContaining({ projectContextMode: "selectable" }),
      undefined,
    );
  });

  it("passes archive context and restore action to the task view", () => {
    const controller = controllerFor("task");
    controller.bootstrap = { surface: "task", shell: VSCODE_SHELL, taskId: "task_1" };
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
    controller.bootstrap = { surface: "task", shell: VSCODE_SHELL, taskId: "task_1" };
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
    controller.state.taskOpenError = {
      taskId: "task_1",
      kind: "failed",
      message: "Connection closed.",
    };

    render(controller);

    expect(surfaceMocks.taskLoading).toHaveBeenCalledWith(
      expect.objectContaining({
        error: "Connection closed.",
        onRetry: controller.retryTaskOpen,
      }),
      undefined,
    );
  });

  it("keeps Web Task Navigation available for a missing routed Task", () => {
    const controller = webControllerFor("task");
    controller.bootstrap = {
      surface: "task",
      shell: WEB_SHELL,
      taskId: "task_unknown",
      appServerConnection: {
        kind: "webProxy",
        endpointUrl: "/__openaide-app-server/probe",
      },
    };
    controller.backendReady = true;
    controller.backendConnectionState = { status: "ready" };
    controller.state.taskOpenError = {
      taskId: "task_unknown",
      kind: "notFound",
      message: "task not found: task_unknown",
    };
    controller.visibleTasks = [snapshot("task_existing").task];

    render(controller);

    expect(surfaceMocks.taskLoading).toHaveBeenCalledWith(
      expect.objectContaining({
        error: "task not found: task_unknown",
        errorKind: "notFound",
      }),
      undefined,
    );
    expect(surfaceMocks.sidebar).toHaveBeenCalledWith(
      expect.objectContaining({
        tasks: controller.visibleTasks,
        taskListError: undefined,
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

  it("does not keep another task's chat on the current task route", () => {
    const controller = webControllerFor("task");
    controller.bootstrap = {
      surface: "task",
      shell: WEB_SHELL,
      taskId: "task_routed",
      appServerConnection: {
        kind: "webProxy",
        endpointUrl: "/__openaide-app-server/probe",
      },
    };
    controller.state.activeTaskId = "task_other";
    controller.state.snapshot = snapshot("task_other");
    controller.state.snapshot.task.title = "why always 24% of cpu taken on this server. always";
    controller.activeTask = controller.state.snapshot.task;

    render(controller);

    expect(surfaceMocks.task).not.toHaveBeenCalled();
    expect(surfaceMocks.taskLoading).toHaveBeenCalled();
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

  it("renders the shared project workbench for Desktop", () => {
    const controller = controllerFor("task");
    controller.bootstrap = {
      surface: "task",
      shell: DESKTOP_SHELL,
    };

    const tree = render(controller);

    expect(surfaceMocks.sidebar).toHaveBeenCalledOnce();
    expect(tree.root.findByProps({ "aria-label": "Open task navigation" })).toBeDefined();
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

function showWorktreeProject(controller: TestController, taskId: string) {
  controller.state.projects = [{
    projectId: "project_1",
    label: "OpenAIDE",
    workspaceRoot: "/workspace/OpenAIDE",
    worktreeRepositoryId: "repository_1",
  }];
  const task = snapshot(taskId).task;
  task.project_id = "project_1";
  task.project_label = "OpenAIDE";
  controller.visibleTasks = [task];
}

function controllerFor(surface: AppController["bootstrap"]["surface"]): TestController {
  const state = createInitialState();
  return {
    activeTask: undefined,
    agents: [],
    backendConnectionState: { status: "connecting" },
    backendReady: false,
    taskMutationReady: false,
    bootstrap: surface === "invalid" ? { surface } : { surface, shell: VSCODE_SHELL },
    callbacks: {
      navigation: {
        archiveNativeSession: vi.fn(),
        archiveOlderTasks: vi.fn(),
        archiveTask: vi.fn(),
        forkNativeSession: vi.fn(),
        forkTask: vi.fn(),
        changeSearch: vi.fn(),
        loadNativeSessions: vi.fn(),
        openNativeSession: vi.fn(),
        openNewTask: vi.fn(),
        openSettings: vi.fn(),
        retryAgent: vi.fn(async () => true),
        openTask: vi.fn(),
      restoreNativeSession: vi.fn(),
      setNativeSessionPinned: vi.fn(),
      setNativeSessionTitle: vi.fn(),
      restoreTask: vi.fn(),
      setTaskPinned: vi.fn(),
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
        cancelAgentAuthentication: vi.fn(),
        logoutAgent: vi.fn(),
        createCustomAgent: vi.fn(),
        deleteCustomAgent: vi.fn(),
        deleteMcpServer: vi.fn(),
        getMcpServerDetails: vi.fn(),
        getSkillDetails: vi.fn(),
        replaceCustomAgent: vi.fn(),
        refreshSettings: vi.fn(),
        saveMcpServer: vi.fn(),
        resetTaskHistory: vi.fn(async () => undefined),
        selectSettingsTab: vi.fn(),
        setAcpTrace: vi.fn(),
        setAgentEnabled: vi.fn(),
        setMcpServerEnabled: vi.fn(),
        setComposerSubmitShortcut: vi.fn(),
        updateCustomAgentMetadata: vi.fn(),
        unlockDeveloperSettings: vi.fn(),
      },
      task: {
        addToQueue: vi.fn(),
        cancel: vi.fn(),
        loadChatPage: vi.fn(),
        loadToolImagePreview: vi.fn(async () => undefined),
        subscribeToolDetail: vi.fn(() => vi.fn()),
        revealAttachment: vi.fn(),
        removeAttachment: vi.fn(),
        removeQueueMessage: vi.fn(),
        takeQueueMessage: vi.fn(),
        moveQueueMessage: vi.fn(),
        sendQueueMessageNow: vi.fn(),
        respondToPermission: vi.fn(),
        respondToQuestion: vi.fn(),
        selectConfigOption: vi.fn(),
        sendPrompt: vi.fn(),
        setPermissionPolicy: vi.fn(async () => undefined),
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
      projects: {
        add: vi.fn(),
        remove: vi.fn(),
        refresh: vi.fn(),
        rename: vi.fn(),
      },
      task: {
        changePrompt: vi.fn(),
        recordScroll: vi.fn(),
        refreshWorkspace: vi.fn(),
        reportAttachmentError: vi.fn(),
      },
    },
    preferences: { composer_submit_shortcut: "mod_enter" },
    retryNativeSessionOpen: vi.fn(),
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
      nativeSessionMutations: state.nativeSessionMutations,
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
    permission_policy: "ask_every_time",
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
      pinned: false,
      updated_at: "2026-05-22T00:00:00.000Z",
      workspace_root: "/workspace",
    },
  };
}
