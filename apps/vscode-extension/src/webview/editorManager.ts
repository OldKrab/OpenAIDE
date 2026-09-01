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
  type SurfaceKind,
  type TaskFocusSource,
  type WebviewBootstrap,
  type WebviewHost,
} from "./types";
import { currentWorkspaceRoot, workspaceRoots, type WorkspaceRoot } from "../workspace/roots";
import { agentTabIcon, newTaskTabIcon, settingsTabIcon } from "./tabIcons";
import { developerSettingsVisible } from "../settings/snapshot";

type PanelBootstrap = Omit<WebviewBootstrap, "shell">;

const MAX_TASK_PANEL_TITLE_LENGTH = 50;
const RETAINED_NEW_TASK_PROJECT_KEY = "openaide.retainedNewTaskProjectId";

export class TaskEditorManager implements vscode.Disposable, WebviewHost, TaskFocusSource {
  private readonly taskPanels = new Map<string, vscode.WebviewPanel>();
  private readonly panelBootstraps = new WeakMap<vscode.WebviewPanel, WebviewBootstrap>();
  private readonly focusedTaskListeners = new Set<(taskId: string | undefined) => void>();
  private focusedPanel: vscode.WebviewPanel | undefined;
  private focusedTaskId: string | undefined;
  private settingsPanel: vscode.WebviewPanel | undefined;
  private newTaskPanel: vscode.WebviewPanel | undefined;
  private readonly nativeSessionPanels = new Map<string, vscode.WebviewPanel>();
  private retainedNewTaskProjectId: string | undefined;

  constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly runtime: RuntimeClient,
    private readonly runtimeProcess: RuntimeProcess,
    private readonly logger: ExtensionLogger,
  ) {
    this.retainedNewTaskProjectId = context.workspaceState.get<string>(RETAINED_NEW_TASK_PROJECT_KEY);
  }

  openNewTask(projectId?: string) {
    const retainedProjectId = this.retainedNewTaskProjectId;
    const effectiveProjectId = projectId ?? retainedProjectId;
    if (projectId) this.retainNewTaskProject(projectId);
    this.logger.info("VS Code New Task Project resolved", {
      project_id: effectiveProjectId,
      project_present: effectiveProjectId !== undefined,
      selection_source: projectId ? "explicit" : retainedProjectId ? "shell_retained" : "app_server_default",
    });
    if (this.newTaskPanel) {
      this.newTaskPanel.reveal(vscode.ViewColumn.Active);
      this.focusPanel(this.newTaskPanel);
      if (effectiveProjectId !== undefined) {
        const current = this.panelBootstraps.get(this.newTaskPanel);
        if (current && current.surface === "task" && !current.taskId) {
          this.panelBootstraps.set(this.newTaskPanel, { ...current, projectId: effectiveProjectId });
          void this.newTaskPanel.webview.postMessage({
            type: "surface.newTaskChanged",
            payload: { project_id: effectiveProjectId },
          });
        }
      }
      return;
    }
    const panel = this.createPanel("openaide.task", "New task", {
      surface: "task",
      // An omitted Project lets the retained/App Server New Task default win.
      // Sidebar Project actions still pass an explicit hint.
      projectId: effectiveProjectId,
    });
    panel.iconPath = newTaskTabIcon(this.context);
    this.newTaskPanel = panel;
    this.focusPanel(panel);
    panel.onDidDispose(() => {
      this.releaseFocusedPanel(panel);
      if (this.newTaskPanel === panel) {
        this.newTaskPanel = undefined;
      }
    });
  }

  /** Shell-owned handoff for New Task selections made in independent webviews. */
  retainNewTaskProject(projectId: string, surface?: SurfaceKind) {
    if (!projectId) return;
    this.retainedNewTaskProjectId = projectId;
    const startedAt = Date.now();
    this.logger.info("VS Code New Task Project retention started", {
      project_id: projectId,
      surface,
    });
    try {
      void Promise.resolve(this.context.workspaceState.update(RETAINED_NEW_TASK_PROJECT_KEY, projectId))
        .then(() => this.logger.info("VS Code New Task Project retention completed", {
          duration_ms: Date.now() - startedAt,
          outcome: "updated",
          project_id: projectId,
          surface,
        }))
        .catch(() => this.logger.warn("VS Code New Task Project retention completed", {
          duration_ms: Date.now() - startedAt,
          error_kind: "workspace_state_update_failed",
          outcome: "failed",
          project_id: projectId,
          surface,
        }));
    } catch {
      this.logger.warn("VS Code New Task Project retention completed", {
        duration_ms: Date.now() - startedAt,
        error_kind: "workspace_state_update_failed",
        outcome: "failed",
        project_id: projectId,
        surface,
      });
    }
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
    panel.iconPath = agentTabIcon(this.context, agentId);
    this.nativeSessionPanels.set(key, panel);
    this.focusPanel(panel);
    panel.onDidDispose(() => {
      this.releaseFocusedPanel(panel);
      if (this.nativeSessionPanels.get(key) === panel) this.nativeSessionPanels.delete(key);
    });
  }

  openTask(taskId: string, title = "Task", agentId?: string) {
    const existing = this.taskPanels.get(taskId);
    if (existing) {
      if (agentId) existing.iconPath = agentTabIcon(this.context, agentId);
      existing.reveal(vscode.ViewColumn.Active);
      this.focusPanel(existing);
      return;
    }
    const panel = this.createPanel("openaide.task", taskPanelTitle(title), { surface: "task", taskId });
    panel.iconPath = agentTabIcon(this.context, agentId);
    this.taskPanels.set(taskId, panel);
    this.focusPanel(panel);
    panel.onDidDispose(() => {
      this.releaseFocusedPanel(panel);
      this.taskPanels.delete(taskId);
    });
  }

  /** Updates an existing Task tab without changing editor focus or opening a new panel. */
  updateTaskTitle(taskId: string, title: string) {
    const panel = this.taskPanels.get(taskId);
    if (panel) panel.title = taskPanelTitle(title);
  }

  openSettings(
    agentId?: string,
    returnToNewTask?: boolean,
    projectId?: string,
    settingsTab?: WebviewBootstrap["settingsTab"],
  ) {
    if (this.settingsPanel) {
      this.settingsPanel.reveal(vscode.ViewColumn.Active);
      this.focusPanel(this.settingsPanel);
      void this.settingsPanel.webview.postMessage({
        type: "surface.settingsChanged",
        payload: {
          ...(agentId ? { agent_id: agentId } : {}),
          ...(returnToNewTask ? { return_to_new_task: true } : {}),
          ...(projectId ? { project_id: projectId } : {}),
          ...(settingsTab ? { settings_tab: settingsTab } : {}),
        },
      });
      return;
    }
    const panel = this.createPanel("openaide.settings", "Settings", {
      surface: "settings",
      settingsAgentId: agentId,
      returnToNewTask,
      projectId,
      settingsTab,
    });
    panel.iconPath = settingsTabIcon(this.context);
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

  updateWorkspaceRoots(roots: WorkspaceRoot[]) {
    const projectIds = roots.map(({ projectId }) => projectId);
    for (const panel of this.panels()) {
      const current = this.panelBootstraps.get(panel);
      if (current) this.panelBootstraps.set(panel, { ...current, projectIds });
      void panel.webview.postMessage({
        type: "surface.workspaceChanged",
        payload: { project_ids: projectIds },
      });
    }
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
    // VS Code owns the webview lifecycle identity. A fresh panel must not
    // inherit another panel's sessionStorage-backed New Task selection.
    const clientInstanceId = randomUUID();
    const viewId = `panel-${clientInstanceId}`;
    this.logger.info("VS Code webview client created", {
      surface: bootstrap.surface,
      client_identity_source: "shell",
      extension_version: extensionVersion(this.context),
    });
    const detachAppServerView = this.runtime.attachAppServerView(viewId, (message) => {
      void panel.webview.postMessage(message);
    });
    panel.onDidDispose(detachAppServerView);
    this.renderPanel(panel, this.bootstrap({ ...bootstrap, clientInstanceId }));
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
        surface: bootstrap.surface,
        developerSettingsStore: this.context.globalState,
        supportExportState: this.context.globalState,
        agentSecretStore: this.context.secrets,
        adoptTask: (taskId, taskTitle, agentId) => this.adoptTaskPanel(panel, taskId, taskTitle, agentId),
        surfaces: this,
      });
    });
    return panel;
  }

  private renderPanel(panel: vscode.WebviewPanel, bootstrap: WebviewBootstrap) {
    this.panelBootstraps.set(panel, bootstrap);
    panel.webview.html = renderWebviewHtml(this.context, panel.webview, bootstrap);
  }

  private adoptTaskPanel(panel: vscode.WebviewPanel, taskId: string, title = "Task", agentId?: string) {
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
    panel.iconPath = agentTabIcon(this.context, agentId);
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

  private panels() {
    return new Set([
      ...this.taskPanels.values(),
      ...this.nativeSessionPanels.values(),
      ...(this.newTaskPanel ? [this.newTaskPanel] : []),
      ...(this.settingsPanel ? [this.settingsPanel] : []),
    ]);
  }

  private bootstrap(bootstrap: PanelBootstrap): WebviewBootstrap {
    return {
      ...bootstrap,
      shell: VSCODE_SHELL,
      // Editor surfaces need the same Project scope as Task Navigation so a
      // multi-root VS Code window can change New Task context in place.
      projectIds: workspaceRoots().map(({ projectId }) => projectId),
      developerSettingsUnlocked: developerSettingsVisible(this.context.globalState),
    };
  }
}

function extensionVersion(context: vscode.ExtensionContext) {
  const version = context.extension.packageJSON.version as unknown;
  return typeof version === "string" ? version : "unknown";
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
