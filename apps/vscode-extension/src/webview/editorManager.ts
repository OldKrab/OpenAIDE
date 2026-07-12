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
import type { WebviewBootstrap, WebviewHost } from "./types";
import { resolveWebviewAppServerConnection } from "./appServerConnection";
import { currentWorkspaceRoot } from "../workspace/roots";

export class TaskEditorManager implements vscode.Disposable, WebviewHost {
  private readonly taskPanels = new Map<string, vscode.WebviewPanel>();
  private readonly panelBootstraps = new WeakMap<vscode.WebviewPanel, WebviewBootstrap>();
  private readonly panelRenderGeneration = new WeakMap<vscode.WebviewPanel, number>();
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
      return;
    }
    const panel = this.createPanel("openaide.task", "New task", {
      surface: "task",
      projectId: projectId ?? currentWorkspaceRoot()?.projectId,
    });
    this.newTaskPanel = panel;
    panel.onDidDispose(() => {
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
      return;
    }
    const panel = this.createPanel("openaide.task", title, { surface: "task", taskId });
    this.taskPanels.set(taskId, panel);
    panel.onDidDispose(() => {
      this.nextPanelGeneration(panel);
      this.taskPanels.delete(taskId);
    });
  }

  openSettings() {
    if (this.settingsPanel) {
      this.settingsPanel.reveal(vscode.ViewColumn.Active);
      return;
    }
    const panel = this.createPanel("openaide.settings", "Settings", { surface: "settings" });
    this.settingsPanel = panel;
    panel.onDidDispose(() => {
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
  }

  private createPanel(viewType: string, title: string, bootstrap: WebviewBootstrap) {
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
    bootstrap: WebviewBootstrap,
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
    panel.title = title.trim() || "Task";
    const current = this.panelBootstraps.get(panel);
    this.panelBootstraps.set(panel, {
      ...current,
      surface: "task",
      taskId,
      projectId: undefined,
    });
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

  private isPanelGenerationCurrent(panel: vscode.WebviewPanel, generation: number) {
    return this.panelRenderGeneration.get(panel) === generation;
  }

  private bootstrap(bootstrap: WebviewBootstrap): WebviewBootstrap {
    return bootstrap;
  }
}
