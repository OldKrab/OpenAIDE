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
import { VSCODE_SHELL, type TaskFocusSource, type WebviewHost } from "./types";
import { resolveWebviewAppServerConnection } from "./appServerConnection";
import { currentWorkspaceRoot } from "../workspace/roots";

export class TaskViewProvider implements vscode.WebviewViewProvider, vscode.Disposable {
  static readonly viewType = "openaide.tasks";
  private view: vscode.WebviewView | undefined;
  private renderGeneration = 0;
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
    this.nextRenderGeneration();
    this.view = undefined;
    this.focusedTaskSubscription.dispose();
  }

  resolveWebviewView(view: vscode.WebviewView) {
    const clientInstanceId = createWebviewClientInstanceId();
    this.view = view;
    view.webview.options = {
      enableScripts: true,
      localResourceRoots: [webviewRoot(this.context)],
    };
    view.webview.html = renderWebviewPreparingHtml(this.context, view.webview);
    void this.renderWhenAppServerReady(view, clientInstanceId, this.nextRenderGeneration());
    view.webview.onDidReceiveMessage((message) =>
      handleWebviewMessage(message, {
        runtime: this.runtime,
        runtimeProcess: this.runtimeProcess,
        post: (payload) => view.webview.postMessage(payload),
        logger: this.logger,
        developerSettingsStore: this.context.globalState,
        agentSecretStore: this.context.secrets,
        surfaces: this.surfaces,
      }),
    );
  }

  private async renderWhenAppServerReady(
    view: vscode.WebviewView,
    clientInstanceId: string,
    generation: number,
  ) {
    try {
      const connection = await resolveWebviewAppServerConnection(
        await this.runtimeProcess.startAppServerConnection(),
      );
      if (!this.isRenderGenerationCurrent(generation) || this.view !== view) return;
      view.webview.html = renderWebviewHtml(this.context, view.webview, {
        ...this.bootstrap(clientInstanceId),
        appServerConnection: connection,
      });
    } catch (error) {
      if (!this.isRenderGenerationCurrent(generation) || this.view !== view) return;
      this.logger.warn("app server handoff failed; rendering without app server connection", { error: String(error) });
      view.webview.html = renderWebviewHtml(this.context, view.webview, this.bootstrap(clientInstanceId));
    }
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

  private bootstrap(clientInstanceId: string): Parameters<typeof renderWebviewHtml>[2] {
    return {
      surface: "navigation",
      shell: VSCODE_SHELL,
      clientInstanceId,
      focusedTaskId: this.focusedTaskId ?? null,
      projectId: currentWorkspaceRoot()?.projectId,
    };
  }

  private nextRenderGeneration() {
    this.renderGeneration += 1;
    return this.renderGeneration;
  }

  private isRenderGenerationCurrent(generation: number) {
    return this.renderGeneration === generation;
  }
}
