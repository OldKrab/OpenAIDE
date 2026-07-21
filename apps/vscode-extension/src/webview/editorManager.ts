import * as vscode from "vscode";
import { randomUUID } from "node:crypto";
import { isAppServerSessionViewMessage } from "@openaide/app-server-client";
import { ExtensionLogger } from "../logging/logger";
import { RuntimeProcess } from "../runtime/process";
import { RuntimeClient } from "../runtime/rpcClient";
import { renderWebviewHtml, webviewRoot } from "./html";
import { handleWebviewMessage } from "./messaging";
import {
  VSCODE_SHELL,
  type TaskFocusSource,
  type WebviewBootstrap,
  type WebviewHost,
} from "./types";
import { currentWorkspaceRoot } from "../workspace/roots";

type PanelBootstrap = Omit<WebviewBootstrap, "shell">;

const MAX_TASK_PANEL_TITLE_LENGTH = 50;

export class TaskEditorManager implements vscode.Disposable, WebviewHost, TaskFocusSource {
  private readonly taskPanels = new Map<string, vscode.WebviewPanel>();
  private readonly panelBootstraps = new WeakMap<vscode.WebviewPanel, WebviewBootstrap>();
  private readonly focusedTaskListeners = new Set<(taskId: string | undefined) => void>();
  private focusedPanel: vscode.WebviewPanel | undefined;
  private focusedTaskId: string | undefined;
  private settingsPanel: vscode.WebviewPanel | undefined;
  private newTaskPanel: vscode.WebviewPanel | undefined;
  private readonly nativeSessionPanels = new Map<string, vscode.WebviewPanel>();

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
      if (this.newTaskPanel === panel) {
        this.newTaskPanel = undefined;
      }
    });
  }

  openNativeSession(agentId: string, nativeSessionId: string, projectId?: string) {
    const key = nativeSessionPanelKey(agentId, nativeSessionId);
    const existing = this.nativeSessionPanels.get(key);
    if (existing) {
      existing.reveal(vscode.ViewColumn.Active);
      this.focusPanel(existing);
      return;
    }
    const panel = this.createPanel("openaide.task", "Opening session…", {
      surface: "nativeSession",
      agentId,
      nativeSessionId,
      projectId: projectId ?? currentWorkspaceRoot()?.projectId,
    });
    this.nativeSessionPanels.set(key, panel);
    this.focusPanel(panel);
    panel.onDidDispose(() => {
      this.releaseFocusedPanel(panel);
      if (this.nativeSessionPanels.get(key) === panel) this.nativeSessionPanels.delete(key);
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
      this.taskPanels.delete(taskId);
    });
  }

  openSettings(agentId?: string, returnToNewTask?: boolean, projectId?: string) {
    if (this.settingsPanel) {
      this.settingsPanel.reveal(vscode.ViewColumn.Active);
      this.focusPanel(this.settingsPanel);
      void this.settingsPanel.webview.postMessage({
        type: "surface.settingsChanged",
        payload: {
          ...(agentId ? { agent_id: agentId } : {}),
          ...(returnToNewTask ? { return_to_new_task: true } : {}),
          ...(projectId ? { project_id: projectId } : {}),
        },
      });
      return;
    }
    const panel = this.createPanel("openaide.settings", "Settings", {
      surface: "settings",
      settingsAgentId: agentId,
      returnToNewTask,
      projectId,
    });
    this.settingsPanel = panel;
    this.focusPanel(panel);
    panel.onDidDispose(() => {
      this.releaseFocusedPanel(panel);
      this.settingsPanel = undefined;
    });
  }

  dispose() {
    this.newTaskPanel?.dispose();
    this.settingsPanel?.dispose();
    for (const panel of this.taskPanels.values()) {
      panel.dispose();
    }
    for (const panel of this.nativeSessionPanels.values()) panel.dispose();
    this.taskPanels.clear();
    this.nativeSessionPanels.clear();
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
    const panel = vscode.window.createWebviewPanel(viewType, title, vscode.ViewColumn.Active, {
      enableScripts: true,
      localResourceRoots: [webviewRoot(this.context)],
      retainContextWhenHidden: true,
    });
    panel.onDidChangeViewState(({ webviewPanel }) => {
      if (webviewPanel.active) {
        this.focusPanel(panel);
      } else {
        this.releaseFocusedPanel(panel);
      }
    });
    const viewId = `panel-${randomUUID()}`;
    const detachAppServerView = this.runtime.attachAppServerView(viewId, (message) => {
      void panel.webview.postMessage(message);
    });
    panel.onDidDispose(detachAppServerView);
    this.renderPanel(panel, this.bootstrap(bootstrap));
    panel.webview.onDidReceiveMessage((message) => {
      if (isAppServerSessionViewMessage(message)) {
        void this.runtime.handleAppServerViewMessage(viewId, message);
        return;
      }
      void handleWebviewMessage(message, {
        runtime: this.runtime,
        runtimeProcess: this.runtimeProcess,
        post: (payload) => panel.webview.postMessage(payload),
        logger: this.logger,
        developerSettingsStore: this.context.globalState,
        agentSecretStore: this.context.secrets,
        adoptTask: (taskId, taskTitle) => this.adoptTaskPanel(panel, taskId, taskTitle),
        surfaces: this,
      });
    });
    return panel;
  }

  private renderPanel(panel: vscode.WebviewPanel, bootstrap: WebviewBootstrap) {
    this.panelBootstraps.set(panel, bootstrap);
    panel.webview.html = renderWebviewHtml(this.context, panel.webview, bootstrap);
  }

  private adoptTaskPanel(panel: vscode.WebviewPanel, taskId: string, title = "Task") {
    const adoptingNewTaskPanel = this.newTaskPanel === panel;
    const current = this.panelBootstraps.get(panel);
    const adoptingNativeSessionPanel = current?.surface === "nativeSession";
    if (!adoptingNewTaskPanel && !adoptingNativeSessionPanel) return;
    const existingTaskPanel = this.taskPanels.get(taskId);
    if (existingTaskPanel && existingTaskPanel !== panel) {
      if (adoptingNewTaskPanel) this.newTaskPanel = undefined;
      if (adoptingNativeSessionPanel && current.agentId && current.nativeSessionId) {
        this.nativeSessionPanels.delete(nativeSessionPanelKey(current.agentId, current.nativeSessionId));
      }
      panel.dispose();
      existingTaskPanel.reveal(vscode.ViewColumn.Active);
      this.focusPanel(existingTaskPanel);
      return;
    }
    panel.title = taskPanelTitle(title);
    this.panelBootstraps.set(panel, {
      ...(current ?? { surface: "task", shell: VSCODE_SHELL }),
      surface: "task",
      taskId,
      projectId: undefined,
      agentId: undefined,
      nativeSessionId: undefined,
    });
    if (panel.active) this.focusPanel(panel);
    if (adoptingNewTaskPanel) this.newTaskPanel = undefined;
    if (adoptingNativeSessionPanel && current.agentId && current.nativeSessionId) {
      this.nativeSessionPanels.delete(nativeSessionPanelKey(current.agentId, current.nativeSessionId));
    }
    if (!this.taskPanels.has(taskId)) {
      this.taskPanels.set(taskId, panel);
      panel.onDidDispose(() => {
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

function nativeSessionPanelKey(agentId: string, nativeSessionId: string) {
  return `${agentId}\u0000${nativeSessionId}`;
}
