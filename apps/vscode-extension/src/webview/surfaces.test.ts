import { beforeEach, describe, expect, it, vi } from "vitest";
import { TaskEditorManager } from "./editorManager";
import { handleWebviewMessage } from "./messaging";
import { TaskViewProvider } from "./navigationProvider";

const vscodeMocks = vi.hoisted(() => ({
  asExternalUri: vi.fn(async (uri: { toString(): string }) => ({
    toString: () => uri.toString(),
  })),
  createWebviewPanel: vi.fn(),
  panels: [] as Array<ReturnType<typeof createPanelMock>>,
}));

vi.mock("vscode", () => ({
  env: {
    asExternalUri: vscodeMocks.asExternalUri,
  },
  Uri: {
    joinPath: (base: { fsPath: string }, ...parts: string[]) => ({ fsPath: [base.fsPath, ...parts].join("/") }),
    parse: (value: string) => ({ toString: () => value }),
  },
  ViewColumn: { Active: 1 },
  window: {
    createWebviewPanel: vscodeMocks.createWebviewPanel,
  },
  workspace: {
    getConfiguration: () => ({ get: vi.fn(() => []) }),
  },
}));

vi.mock("./messaging", () => ({
  handleWebviewMessage: vi.fn(),
}));

vi.mock("../workspace/roots", () => ({
  currentWorkspaceRoot: () => ({ label: "OpenAIDE", path: "/workspace/OpenAIDE", projectId: "project-current" }),
}));

describe("VS Code webview surfaces", () => {
  beforeEach(() => {
    vscodeMocks.asExternalUri.mockReset();
    vscodeMocks.asExternalUri.mockImplementation(async (uri: { toString(): string }) => ({
      toString: () => uri.toString(),
    }));
    vscodeMocks.panels.length = 0;
    vscodeMocks.createWebviewPanel.mockReset();
    vi.mocked(handleWebviewMessage).mockReset();
    vi.mocked(handleWebviewMessage).mockImplementation(async (message, context: {
      adoptTask?: (taskId: string, title?: string) => void;
      surfaces?: { openTask: (taskId: string, title?: string) => void };
    }) => {
      if (!isObject(message) || message.type !== "surface.openTask" || !isObject(message.payload)) return;
      const taskId = typeof message.payload.task_id === "string" ? message.payload.task_id : undefined;
      if (!taskId) return;
      const title = typeof message.payload.title === "string" ? message.payload.title : undefined;
      context.adoptTask?.(taskId, title);
      context.surfaces?.openTask(taskId, title);
    });
    vscodeMocks.createWebviewPanel.mockImplementation(() => {
      const panel = createPanelMock();
      vscodeMocks.panels.push(panel);
      return panel;
    });
  });

  it("boots the activity sidebar as navigation only", () => {
    const view = createViewMock();
    const provider = new TaskViewProvider(context(), runtime(), runtimeProcess(), logger(), surfaces());

    provider.resolveWebviewView(view as never);

    expect(view.webview.html).toContain('data-surface="navigation"');
    expect(view.webview.html).toContain('data-shell="vscodeExtension"');
    expect(view.webview.html).toContain('data-navigation-mode="currentProject"');
    expect(view.webview.html).toContain('data-task-id=""');
    expect(view.webview.html).toContain('data-project-id="project-current"');
    expect(view.webview.html).toContain('data-composer-submit-shortcut="enter"');
    expect(view.webview.html).not.toContain('data-surface="task"');
    expect(view.webview.html).not.toContain('data-surface="settings"');
  });

  it("does not let disposal of a replaced navigation view detach its replacement", () => {
    const broker = runtime();
    const firstStop = vi.fn();
    const secondStop = vi.fn();
    broker.attachAppServerView
      .mockReturnValueOnce(firstStop)
      .mockReturnValueOnce(secondStop);
    const provider = new TaskViewProvider(context(), broker, runtimeProcess(), logger(), surfaces());
    const first = createViewMock();
    const second = createViewMock();

    provider.resolveWebviewView(first as never);
    provider.resolveWebviewView(second as never);
    triggerViewDispose(first);

    expect(firstStop).toHaveBeenCalledOnce();
    expect(secondStop).not.toHaveBeenCalled();
    triggerViewDispose(second);
    expect(secondStop).toHaveBeenCalledOnce();
  });

  it("opens new task, existing task, and settings as editor webview panels", () => {
    const manager = new TaskEditorManager(context(), runtime(), runtimeProcess(), logger());

    manager.openNewTask();
    manager.openTask("task_1", "Fix ACP");
    manager.openSettings();

    expect(vscodeMocks.createWebviewPanel).toHaveBeenNthCalledWith(
      1,
      "openaide.task",
      "New task",
      1,
      expect.objectContaining({ enableScripts: true, retainContextWhenHidden: true }),
    );
    expect(vscodeMocks.panels[0].webview.html).toContain('data-surface="task"');
    expect(vscodeMocks.panels[0].webview.html).toContain('data-task-id=""');
    expect(vscodeMocks.panels[0].webview.html).toContain('data-project-id="project-current"');
    expect(vscodeMocks.panels[0].webview.html).toContain('data-composer-submit-shortcut="enter"');

    expect(vscodeMocks.createWebviewPanel).toHaveBeenNthCalledWith(
      2,
      "openaide.task",
      "Fix ACP",
      1,
      expect.objectContaining({ enableScripts: true, retainContextWhenHidden: true }),
    );
    expect(vscodeMocks.panels[1].webview.html).toContain('data-surface="task"');
    expect(vscodeMocks.panels[1].webview.html).toContain('data-task-id="task_1"');

    expect(vscodeMocks.createWebviewPanel).toHaveBeenNthCalledWith(
      3,
      "openaide.settings",
      "Settings",
      1,
      expect.objectContaining({ enableScripts: true, retainContextWhenHidden: true }),
    );
    expect(vscodeMocks.panels[2].webview.html).toContain('data-surface="settings"');
  });

  it("keeps Task editor tab labels concise without changing the Task title", () => {
    const manager = new TaskEditorManager(context(), runtime(), runtimeProcess(), logger());
    const fullTitle = "Read-only bounded lookup in upstream OpenCode at /home/example/provider-support";

    manager.openTask("task_1", fullTitle);

    expect(vscodeMocks.createWebviewPanel).toHaveBeenCalledWith(
      "openaide.task",
      "Read-only bounded lookup in upstream OpenCode at…",
      1,
      expect.any(Object),
    );
  });

  it("keeps the editor tab concise when a New Task becomes a Task", () => {
    const manager = new TaskEditorManager(context(), runtime(), runtimeProcess(), logger());

    manager.openNewTask();
    const panel = vscodeMocks.panels[0];
    triggerLastMessageHandler(panel, {
      type: "surface.openTask",
      payload: {
        task_id: "created_task",
        title: "Read-only bounded lookup in upstream OpenCode at /home/example/provider-support",
      },
    });

    expect(panel.title).toBe("Read-only bounded lookup in upstream OpenCode at…");
  });

  it("attaches task webviews to the host broker without exposing client credentials", () => {
    const broker = runtime();
    const manager = new TaskEditorManager(context(), broker, runtimeProcess(), logger());

    manager.openNewTask();
    manager.openTask("task_1", "Fix ACP");

    expect(broker.attachAppServerView).toHaveBeenCalledTimes(2);
    for (const panel of vscodeMocks.panels) {
      expect(panel.webview.html).not.toContain("data-client-instance-id");
      expect(panel.webview.html).not.toContain("data-app-server-connection");
      expect(panel.webview.html).not.toContain("authToken");
    }
  });

  it("routes typed App Server session messages through the extension host client", () => {
    const broker = runtime();
    const manager = new TaskEditorManager(context(), broker, runtimeProcess(), logger());

    manager.openNewTask();
    const panel = vscodeMocks.panels[0];
    triggerLastMessageHandler(panel, {
      type: "appServer.session.initialize",
      requestId: "initialize-1",
    });

    expect(broker.handleAppServerViewMessage).toHaveBeenCalledWith(
      expect.stringMatching(/^panel-/),
      { type: "appServer.session.initialize", requestId: "initialize-1" },
    );
    expect(handleWebviewMessage).not.toHaveBeenCalled();
  });

  it("reveals existing editor tabs instead of creating duplicate panels", () => {
    const manager = new TaskEditorManager(context(), runtime(), runtimeProcess(), logger());

    manager.openNewTask();
    manager.openNewTask();
    manager.openTask("task_1", "Fix ACP");
    manager.openTask("task_1", "Fix ACP");
    manager.openSettings();
    manager.openSettings();

    expect(vscodeMocks.createWebviewPanel).toHaveBeenCalledTimes(3);
    expect(vscodeMocks.panels[0].reveal).toHaveBeenCalledWith(1);
    expect(vscodeMocks.panels[1].reveal).toHaveBeenCalledWith(1);
    expect(vscodeMocks.panels[2].reveal).toHaveBeenCalledWith(1);
  });

  it("keeps task navigation focused on the active Task editor tab", () => {
    const manager = new TaskEditorManager(context(), runtime(), runtimeProcess(), logger());
    const view = createViewMock();
    const provider = new TaskViewProvider(context(), runtime(), runtimeProcess(), logger(), manager);
    provider.resolveWebviewView(view as never);

    manager.openTask("task_1", "First task");
    manager.openTask("task_2", "Second task");
    triggerViewState(vscodeMocks.panels[0], true);

    expect(view.webview.postMessage).toHaveBeenLastCalledWith({
      type: "surface.focusChanged",
      payload: { task_id: "task_1" },
    });

    triggerViewState(vscodeMocks.panels[0], false);
    expect(view.webview.postMessage).toHaveBeenLastCalledWith({
      type: "surface.focusChanged",
      payload: {},
    });
  });

  it("bootstraps task navigation with the already-focused Task editor", () => {
    const manager = new TaskEditorManager(context(), runtime(), runtimeProcess(), logger());
    manager.openTask("task_1", "Focused task");
    const view = createViewMock();
    const provider = new TaskViewProvider(context(), runtime(), runtimeProcess(), logger(), manager);

    provider.resolveWebviewView(view as never);

    expect(view.webview.html).toContain('data-focused-task-id="&quot;task_1&quot;"');
  });

  it("reuses the registered Task panel when a New Task routes to the same Task", () => {
    const manager = new TaskEditorManager(context(), runtime(), runtimeProcess(), logger());

    manager.openTask("task_1", "Existing task");
    const existingTaskPanel = vscodeMocks.panels[0];
    manager.openNewTask();
    const supersededNewTaskPanel = vscodeMocks.panels[1];

    triggerLastMessageHandler(supersededNewTaskPanel, {
      type: "surface.openTask",
      payload: { task_id: "task_1", title: "Existing task" },
    });

    expect(supersededNewTaskPanel.dispose).toHaveBeenCalledOnce();
    expect(existingTaskPanel.dispose).not.toHaveBeenCalled();
    expect(existingTaskPanel.reveal).toHaveBeenCalledWith(1);
    expect(supersededNewTaskPanel.webview.postMessage).not.toHaveBeenCalledWith({
      type: "surface.routeChanged",
      payload: { surface: "task", task_id: "task_1" },
    });

    manager.openTask("task_1", "Existing task");
    expect(vscodeMocks.createWebviewPanel).toHaveBeenCalledTimes(2);
    manager.openNewTask();
    expect(vscodeMocks.createWebviewPanel).toHaveBeenCalledTimes(3);
  });

  it("keeps adopted task panels from clearing a newer New Task panel", () => {
    const manager = new TaskEditorManager(context(), runtime(), runtimeProcess(), logger());

    manager.openNewTask();
    const firstPanel = vscodeMocks.panels[0];
    triggerLastMessageHandler(firstPanel, {
      type: "surface.openTask",
      payload: { task_id: "created_task", title: "Created task" },
    });
    manager.openNewTask();
    const secondPanel = vscodeMocks.panels[1];

    triggerFirstDisposeHandler(firstPanel);
    manager.openNewTask();

    expect(vscodeMocks.createWebviewPanel).toHaveBeenCalledTimes(2);
    expect(secondPanel.reveal).toHaveBeenCalledWith(1);
  });

  it("adopts a successful New Task in place through the production surface route", () => {
    const manager = new TaskEditorManager(context(), runtime(), runtimeProcess(), logger());

    manager.openNewTask();
    const panel = vscodeMocks.panels[0];
    const originalHtml = panel.webview.html;
    expect(panel.webview.html).toContain('data-task-id=""');

    triggerLastMessageHandler(panel, {
      type: "surface.openTask",
      payload: { task_id: "created_task", title: "Created task" },
    });

    expect(panel.title).toBe("Created task");
    expect(panel.webview.html).toBe(originalHtml);
    expect(panel.webview.postMessage).toHaveBeenCalledWith({
      type: "surface.routeChanged",
      payload: { surface: "task", task_id: "created_task" },
    });
    manager.openTask("created_task", "Created task");
    expect(vscodeMocks.createWebviewPanel).toHaveBeenCalledTimes(1);
    expect(panel.reveal).toHaveBeenCalledWith(1);

    manager.openNewTask();
    expect(vscodeMocks.createWebviewPanel).toHaveBeenCalledTimes(2);
  });

});

function context() {
  return {
    extensionUri: { fsPath: "/extension" },
  } as never;
}

function runtime() {
  return {
    attachAppServerView: vi.fn(() => vi.fn()),
    handleAppServerViewMessage: vi.fn(async () => true),
    createTask: vi.fn().mockResolvedValue({
      task: {
        task_id: "created_task",
        title: "Created task",
      },
    }),
  } as never;
}

function runtimeProcess(connection?: Promise<unknown>) {
  return {
    startAppServerConnection: connection ? vi.fn(() => connection) : undefined,
  } as never;
}

function logger() {
  return { warn: vi.fn(), info: vi.fn() } as never;
}

function surfaces() {
  return {
    currentFocusedTaskId: vi.fn(() => undefined),
    onDidChangeFocusedTask: vi.fn(() => ({ dispose: vi.fn() })),
    openNewTask: vi.fn(),
    openSettings: vi.fn(),
    openTask: vi.fn(),
  };
}

function createViewMock() {
  return {
    webview: createWebviewMock(),
    onDidDispose: vi.fn(),
  };
}

function createPanelMock() {
  return {
    active: true,
    title: "",
    webview: createWebviewMock(),
    onDidChangeViewState: vi.fn(),
    onDidDispose: vi.fn(),
    reveal: vi.fn(),
    dispose: vi.fn(),
  };
}

function createWebviewMock() {
  return {
    cspSource: "vscode-resource:",
    html: "",
    options: undefined,
    asWebviewUri: vi.fn((uri: { fsPath: string }) => `webview:${uri.fsPath}`),
    onDidReceiveMessage: vi.fn(),
    postMessage: vi.fn(async () => true),
  };
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function triggerLastMessageHandler(panel: ReturnType<typeof createPanelMock>, message: unknown) {
  const handler = panel.webview.onDidReceiveMessage.mock.calls.at(-1)?.[0];
  if (!handler) throw new Error("missing message handler");
  handler(message);
}

function triggerFirstDisposeHandler(panel: ReturnType<typeof createPanelMock>) {
  const handler = panel.onDidDispose.mock.calls[0]?.[0];
  if (!handler) throw new Error("missing dispose handler");
  handler();
}

function triggerViewDispose(view: ReturnType<typeof createViewMock>) {
  const handler = view.onDidDispose.mock.calls[0]?.[0];
  if (!handler) throw new Error("missing view dispose handler");
  handler();
}

function triggerViewState(panel: ReturnType<typeof createPanelMock>, active: boolean) {
  panel.active = active;
  const handler = panel.onDidChangeViewState.mock.calls[0]?.[0];
  if (!handler) throw new Error("missing view-state handler");
  handler({ webviewPanel: panel });
}
