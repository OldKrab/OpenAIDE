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
import type { WebviewHost } from "./types";
import { resolveWebviewAppServerConnection } from "./appServerConnection";
import { currentWorkspaceRoot } from "../workspace/roots";

export class TaskViewProvider implements vscode.WebviewViewProvider {
  static readonly viewType = "openaide.tasks";
  private view: vscode.WebviewView | undefined;
  private renderGeneration = 0;

  constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly runtime: RuntimeClient,
    private readonly runtimeProcess: RuntimeProcess,
    private readonly logger: ExtensionLogger,
    private readonly surfaces: WebviewHost,
  ) {}

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

  private bootstrap(clientInstanceId: string): Parameters<typeof renderWebviewHtml>[2] {
    return {
      surface: "navigation",
      clientInstanceId,
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
