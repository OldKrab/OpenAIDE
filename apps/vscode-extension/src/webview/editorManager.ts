import * as vscode from "vscode";
import { ExtensionLogger } from "../logging/logger";
import { RuntimeProcess } from "../runtime/process";
import { RuntimeClient } from "../runtime/rpcClient";
import {
  createWebviewClientInstanceId,
  renderWebviewHtml,
  renderWebviewPreparingHtml,
  webviewRoot,
} from "./html";
import { handleWebviewMessage } from "./messaging";
import {
  VSCODE_SHELL,
  type TaskFocusSource,
  type WebviewBootstrap,
  type WebviewHost,
} from "./types";
import { resolveWebviewAppServerConnection } from "./appServerConnection";
import { currentWorkspaceRoot } from "../workspace/roots";

type PanelBootstrap = Omit<WebviewBootstrap, "shell">;

const MAX_TASK_PANEL_TITLE_LENGTH = 50;

export class TaskEditorManager implements vscode.Disposable, WebviewHost, TaskFocusSource {
  private readonly taskPanels = new Map<string, vscode.WebviewPanel>();
  private readonly panelBootstraps = new WeakMap<vscode.WebviewPanel, WebviewBootstrap>();
  private readonly panelRenderGeneration = new WeakMap<vscode.WebviewPanel, number>();
  private readonly focusedTaskListeners = new Set<(taskId: string | undefined) => void>();
  private focusedPanel: vscode.WebviewPanel | undefined;
  private focusedTaskId: string | undefined;
  private settingsPanel: vscode.WebviewPanel | undefined;
  private newTaskPanel: vscode.WebviewPanel | undefined;

  constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly runtime: RuntimeClient,
    private readonly runtimeProcess: RuntimeProcess,
    private readonly logger: ExtensionLogger,
  ) {}

  openNewTask(projectId?: string) {
    if (this.newTaskPanel) {
      this.newTaskPanel.reveal(vscode.ViewColumn.Active);
      this.focusPanel(this.newTaskPanel);
      return;
    }
    const panel = this.createPanel("openaide.task", "New task", {
      surface: "task",
      projectId: projectId ?? currentWorkspaceRoot()?.projectId,
    });
    this.newTaskPanel = panel;
    this.focusPanel(panel);
    panel.onDidDispose(() => {
      this.releaseFocusedPanel(panel);
      this.nextPanelGeneration(panel);
      if (this.newTaskPanel === panel) {
        this.newTaskPanel = undefined;
      }
    });
  }

  openTask(taskId: string, title = "Task") {
    const existing = this.taskPanels.get(taskId);
    if (existing) {
      existing.reveal(vscode.ViewColumn.Active);
      this.focusPanel(existing);
      return;
    }
    const panel = this.createPanel("openaide.task", taskPanelTitle(title), { surface: "task", taskId });
    this.taskPanels.set(taskId, panel);
    this.focusPanel(panel);
    panel.onDidDispose(() => {
      this.releaseFocusedPanel(panel);
      this.nextPanelGeneration(panel);
      this.taskPanels.delete(taskId);
    });
  }

  openSettings() {
    if (this.settingsPanel) {
      this.settingsPanel.reveal(vscode.ViewColumn.Active);
      this.focusPanel(this.settingsPanel);
      return;
    }
    const panel = this.createPanel("openaide.settings", "Settings", { surface: "settings" });
    this.settingsPanel = panel;
    this.focusPanel(panel);
    panel.onDidDispose(() => {
      this.releaseFocusedPanel(panel);
      this.nextPanelGeneration(panel);
      this.settingsPanel = undefined;
    });
  }

  dispose() {
    this.newTaskPanel?.dispose();
    this.settingsPanel?.dispose();
    for (const panel of this.taskPanels.values()) {
      panel.dispose();
    }
    this.taskPanels.clear();
    this.focusedTaskListeners.clear();
  }

  currentFocusedTaskId() {
    return this.focusedTaskId;
  }

  onDidChangeFocusedTask(listener: (taskId: string | undefined) => void) {
    this.focusedTaskListeners.add(listener);
    return { dispose: () => this.focusedTaskListeners.delete(listener) };
  }

  private createPanel(viewType: string, title: string, bootstrap: PanelBootstrap) {
    // Panels of one view type share browser storage, so the host owns per-panel connection identity.
    const panelBootstrap = {
      ...bootstrap,
      clientInstanceId: createWebviewClientInstanceId(),
    };
    const panel = vscode.window.createWebviewPanel(viewType, title, vscode.ViewColumn.Active, {
      enableScripts: true,
      localResourceRoots: [webviewRoot(this.context)],
      retainContextWhenHidden: true,
    });
    this.panelBootstraps.set(panel, this.bootstrap(panelBootstrap));
    panel.onDidChangeViewState(({ webviewPanel }) => {
      if (webviewPanel.active) {
        this.focusPanel(panel);
      } else {
        this.releaseFocusedPanel(panel);
      }
    });
    panel.webview.html = renderWebviewPreparingHtml(this.context, panel.webview);
    void this.renderPanelWhenAppServerReady(panel, panelBootstrap, this.nextPanelGeneration(panel));
    panel.webview.onDidReceiveMessage((message) =>
      handleWebviewMessage(message, {
        runtime: this.runtime,
        runtimeProcess: this.runtimeProcess,
        post: (payload) => panel.webview.postMessage(payload),
        logger: this.logger,
        developerSettingsStore: this.context.globalState,
        agentSecretStore: this.context.secrets,
        adoptTask: (taskId, taskTitle) => this.adoptTaskPanel(panel, taskId, taskTitle),
        surfaces: this,
      }),
    );
    return panel;
  }

  private async renderPanelWhenAppServerReady(
    panel: vscode.WebviewPanel,
    bootstrap: PanelBootstrap,
    generation: number,
  ) {
    try {
      const connection = await resolveWebviewAppServerConnection(
        await this.runtimeProcess.startAppServerConnection(),
      );
      if (!this.isPanelGenerationCurrent(panel, generation)) return;
      this.renderPanel(panel, {
        ...this.bootstrap(bootstrap),
        appServerConnection: connection,
      });
    } catch (error) {
      if (!this.isPanelGenerationCurrent(panel, generation)) return;
      this.logger.warn("app server handoff failed; rendering without app server connection", { error: String(error) });
      this.renderPanel(panel, this.bootstrap(bootstrap));
    }
  }

  private renderPanel(panel: vscode.WebviewPanel, bootstrap: WebviewBootstrap) {
    this.panelBootstraps.set(panel, bootstrap);
    panel.webview.html = renderWebviewHtml(this.context, panel.webview, bootstrap);
  }

  private adoptTaskPanel(panel: vscode.WebviewPanel, taskId: string, title = "Task") {
    const adoptingNewTaskPanel = this.newTaskPanel === panel;
    if (!adoptingNewTaskPanel) return;
    const existingTaskPanel = this.taskPanels.get(taskId);
    if (existingTaskPanel && existingTaskPanel !== panel) {
      this.newTaskPanel = undefined;
      // Invalidate pending bootstrap work before closing its superseded Backend client.
      this.nextPanelGeneration(panel);
      panel.dispose();
      existingTaskPanel.reveal(vscode.ViewColumn.Active);
      this.focusPanel(existingTaskPanel);
      return;
    }
    panel.title = taskPanelTitle(title);
    const current = this.panelBootstraps.get(panel);
    this.panelBootstraps.set(panel, {
      ...(current ?? { surface: "task", shell: VSCODE_SHELL }),
      surface: "task",
      taskId,
      projectId: undefined,
    });
    if (panel.active) this.focusPanel(panel);
    this.newTaskPanel = undefined;
    if (!this.taskPanels.has(taskId)) {
      this.taskPanels.set(taskId, panel);
      panel.onDidDispose(() => {
        this.nextPanelGeneration(panel);
        if (this.taskPanels.get(taskId) === panel) {
          this.taskPanels.delete(taskId);
        }
      });
    }
    void panel.webview.postMessage({
      type: "surface.routeChanged",
      payload: { surface: "task", task_id: taskId },
    });
  }

  private nextPanelGeneration(panel: vscode.WebviewPanel) {
    const next = (this.panelRenderGeneration.get(panel) ?? 0) + 1;
    this.panelRenderGeneration.set(panel, next);
    return next;
  }

  /** Publishes editor focus only when the shell-visible Task identity changes. */
  private focusPanel(panel: vscode.WebviewPanel) {
    this.focusedPanel = panel;
    const bootstrap = this.panelBootstraps.get(panel);
    this.publishFocusedTask(bootstrap?.surface === "task" ? bootstrap.taskId : undefined);
  }

  private releaseFocusedPanel(panel: vscode.WebviewPanel) {
    if (this.focusedPanel !== panel) return;
    this.focusedPanel = undefined;
    this.publishFocusedTask(undefined);
  }

  private publishFocusedTask(taskId: string | undefined) {
    if (this.focusedTaskId === taskId) return;
    this.focusedTaskId = taskId;
    for (const listener of this.focusedTaskListeners) listener(taskId);
  }

  private isPanelGenerationCurrent(panel: vscode.WebviewPanel, generation: number) {
    return this.panelRenderGeneration.get(panel) === generation;
  }

  private bootstrap(bootstrap: PanelBootstrap): WebviewBootstrap {
    return { ...bootstrap, shell: VSCODE_SHELL };
  }
}

/** Keeps the native VS Code tab navigable while the Task retains its complete title. */
function taskPanelTitle(title: string) {
  const normalized = title.trim() || "Task";
  if (normalized.length <= MAX_TASK_PANEL_TITLE_LENGTH) return normalized;
  return `${normalized.slice(0, MAX_TASK_PANEL_TITLE_LENGTH - 1).trimEnd()}…`;
}
