import * as vscode from "vscode";
import { randomUUID } from "node:crypto";
import { isAppServerSessionViewMessage } from "@openaide/app-server-client";
import { ExtensionLogger } from "../logging/logger";
import { RuntimeProcess } from "../runtime/process";
import { RuntimeClient } from "../runtime/rpcClient";
import { renderWebviewHtml, webviewRoot } from "./html";
import { handleWebviewMessage } from "./messaging";
import { VSCODE_SHELL, type TaskFocusSource, type WebviewHost } from "./types";
import { currentWorkspaceRoot } from "../workspace/roots";

export class TaskViewProvider implements vscode.WebviewViewProvider, vscode.Disposable {
  static readonly viewType = "openaide.tasks";
  private view: vscode.WebviewView | undefined;
  private detachAppServerView: (() => void) | undefined;
  private readonly focusedTaskSubscription: { dispose: () => void };
  private focusedTaskId: string | undefined;

  constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly runtime: RuntimeClient,
    private readonly runtimeProcess: RuntimeProcess,
    private readonly logger: ExtensionLogger,
    private readonly surfaces: WebviewHost & TaskFocusSource,
  ) {
    this.focusedTaskId = surfaces.currentFocusedTaskId();
    this.focusedTaskSubscription = surfaces.onDidChangeFocusedTask((taskId) => {
      this.focusedTaskId = taskId;
      void this.publishFocusedTask(taskId);
    });
  }

  dispose() {
    this.detachAppServerView?.();
    this.detachAppServerView = undefined;
    this.view = undefined;
    this.focusedTaskSubscription.dispose();
  }

  resolveWebviewView(view: vscode.WebviewView) {
    this.detachAppServerView?.();
    this.view = view;
    view.webview.options = {
      enableScripts: true,
      localResourceRoots: [webviewRoot(this.context)],
    };
    const viewId = `navigation-${randomUUID()}`;
    const detachAppServerView = this.runtime.attachAppServerView(viewId, (message) => {
      void view.webview.postMessage(message);
    });
    this.detachAppServerView = detachAppServerView;
    view.onDidDispose(() => {
      if (this.detachAppServerView === detachAppServerView) {
        detachAppServerView();
        this.detachAppServerView = undefined;
      }
      if (this.view === view) this.view = undefined;
    });
    view.webview.html = renderWebviewHtml(this.context, view.webview, this.bootstrap());
    view.webview.onDidReceiveMessage((message) => {
      if (isAppServerSessionViewMessage(message)) {
        void this.runtime.handleAppServerViewMessage(viewId, message);
        return;
      }
      void handleWebviewMessage(message, {
        runtime: this.runtime,
        runtimeProcess: this.runtimeProcess,
        post: (payload) => view.webview.postMessage(payload),
        logger: this.logger,
        developerSettingsStore: this.context.globalState,
        agentSecretStore: this.context.secrets,
        surfaces: this.surfaces,
      });
    });
  }

  private async publishFocusedTask(taskId: string | undefined) {
    const view = this.view;
    if (!view) return;
    try {
      await view.webview.postMessage({
        type: "surface.focusChanged",
        payload: taskId ? { task_id: taskId } : {},
      });
    } catch (error) {
      this.logger.warn("failed to publish focused Task to task navigation", {
        error: String(error),
      });
    }
  }

  private bootstrap(): Parameters<typeof renderWebviewHtml>[2] {
    return {
      surface: "navigation",
      shell: VSCODE_SHELL,
      focusedTaskId: this.focusedTaskId ?? null,
      projectId: currentWorkspaceRoot()?.projectId,
    };
  }
}
