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
    expect(view.webview.html).toContain('data-task-id=""');
    expect(view.webview.html).toContain('data-project-id="project-current"');
    expect(view.webview.html).toContain('data-composer-submit-shortcut="enter"');
    expect(view.webview.html).not.toContain('data-surface="task"');
    expect(view.webview.html).not.toContain('data-surface="settings"');
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

  it("gives task webviews with the same VS Code origin distinct client identities", () => {
    const manager = new TaskEditorManager(context(), runtime(), runtimeProcess(), logger());

    manager.openNewTask();
    manager.openTask("task_1", "Fix ACP");

    const newTaskClientId = dataAttribute(vscodeMocks.panels[0].webview.html, "client-instance-id");
    const taskClientId = dataAttribute(vscodeMocks.panels[1].webview.html, "client-instance-id");
    expect(newTaskClientId).toBeTruthy();
    expect(taskClientId).toBeTruthy();
    expect(taskClientId).not.toBe(newTaskClientId);
  });

  it("renders a preparing shell until App Server handoff supplies bootstrap connection info", async () => {
    const handoff = deferredConnection();
    const manager = new TaskEditorManager(context(), runtime(), runtimeProcess(handoff.promise), logger());

    manager.openNewTask();
    const panel = vscodeMocks.panels[0];

    expect(panel.webview.html).toContain("Preparing OpenAIDE");
    handoff.resolve({
      kind: "localHttp",
      endpointUrl: "http://127.0.0.1:1234/probe",
      authToken: "token-1",
    });
    await settle();

    expect(panel.webview.html).toContain('data-surface="task"');
    expect(panel.webview.html).toContain("&quot;endpointUrl&quot;:&quot;http://127.0.0.1:1234/probe&quot;");
    expect(panel.webview.html).toContain("&quot;authToken&quot;:&quot;token-1&quot;");
  });

  it("forwards the remote App Server endpoint before bootstrapping an editor webview", async () => {
    vscodeMocks.asExternalUri.mockResolvedValue({
      toString: () => "http://127.0.0.1:54321/probe",
    });
    const manager = new TaskEditorManager(
      context(),
      runtime(),
      runtimeProcess(Promise.resolve({
        kind: "localHttp",
        endpointUrl: "http://127.0.0.1:1234/probe",
        authToken: "token-1",
      })),
      logger(),
    );

    manager.openNewTask();
    await settle();

    expect(vscodeMocks.asExternalUri).toHaveBeenCalledWith(
      expect.objectContaining({ toString: expect.any(Function) }),
    );
    expect(vscodeMocks.panels[0].webview.html).toContain(
      "&quot;endpointUrl&quot;:&quot;http://127.0.0.1:54321/probe&quot;",
    );
    expect(vscodeMocks.panels[0].webview.html).not.toContain(
      "&quot;endpointUrl&quot;:&quot;http://127.0.0.1:1234/probe&quot;",
    );
  });

  it("forwards the remote App Server endpoint before bootstrapping task navigation", async () => {
    vscodeMocks.asExternalUri.mockResolvedValue({
      toString: () => "http://127.0.0.1:54321/probe",
    });
    const view = createViewMock();
    const provider = new TaskViewProvider(
      context(),
      runtime(),
      runtimeProcess(Promise.resolve({
        kind: "localHttp",
        endpointUrl: "http://127.0.0.1:1234/probe",
        authToken: "token-1",
      })),
      logger(),
      surfaces(),
    );

    provider.resolveWebviewView(view as never);
    await settle();

    expect(view.webview.html).toContain(
      "&quot;endpointUrl&quot;:&quot;http://127.0.0.1:54321/probe&quot;",
    );
  });

  it("renders without App Server connection when handoff fails", async () => {
    const manager = new TaskEditorManager(
      context(),
      runtime(),
      runtimeProcess(Promise.reject(new Error("handoff failed"))),
      logger(),
    );

    manager.openNewTask();
    await settle();

    expect(vscodeMocks.panels[0].webview.html).toContain('data-surface="task"');
    expect(vscodeMocks.panels[0].webview.html).toContain('data-app-server-connection="null"');
  });

  it("does not render into a disposed panel after handoff resolves", async () => {
    const handoff = deferredConnection();
    const manager = new TaskEditorManager(context(), runtime(), runtimeProcess(handoff.promise), logger());

    manager.openNewTask();
    const panel = vscodeMocks.panels[0];
    const preparingHtml = panel.webview.html;
    triggerFirstDisposeHandler(panel);
    handoff.resolve({
      kind: "localHttp",
      endpointUrl: "http://127.0.0.1:1234/probe",
      authToken: "token-1",
    });
    await settle();

    expect(panel.webview.html).toBe(preparingHtml);
  });

  it("preserves LocalHttp bootstrap when adopting a New Task panel", async () => {
    const handoff = deferredConnection();
    const manager = new TaskEditorManager(context(), runtime(), runtimeProcess(handoff.promise), logger());

    manager.openNewTask();
    const panel = vscodeMocks.panels[0];
    handoff.resolve({
      kind: "localHttp",
      endpointUrl: "http://127.0.0.1:1234/probe",
      authToken: "token-1",
    });
    await settle();
    const originalHtml = panel.webview.html;

    triggerLastMessageHandler(panel, {
      type: "surface.openTask",
      payload: { task_id: "created_task", title: "Created task" },
    });

    expect(panel.title).toBe("Created task");
    expect(panel.webview.html).toBe(originalHtml);
    expect(panel.webview.html).toContain("&quot;endpointUrl&quot;:&quot;http://127.0.0.1:1234/probe&quot;");
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
    openNewTask: vi.fn(),
    openSettings: vi.fn(),
    openTask: vi.fn(),
  };
}

function createViewMock() {
  return {
    webview: createWebviewMock(),
  };
}

function createPanelMock() {
  return {
    title: "",
    webview: createWebviewMock(),
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

function dataAttribute(html: string, name: string) {
  return new RegExp(`data-${name}="([^"]*)"`).exec(html)?.[1];
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

function deferredConnection() {
  let resolve!: (value: {
    kind: "localHttp";
    endpointUrl: string;
    authToken: string;
  }) => void;
  const promise = new Promise<{
    kind: "localHttp";
    endpointUrl: string;
    authToken: string;
  }>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

async function settle() {
  await Promise.resolve();
  await Promise.resolve();
}
