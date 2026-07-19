import * as vscode from "vscode";
import { randomUUID } from "node:crypto";
import { isAppServerSessionViewMessage } from "@openaide/app-server-client";
import { ExtensionLogger } from "../logging/logger";
import { RuntimeProcess } from "../runtime/process";
import { RuntimeClient } from "../runtime/rpcClient";
import {
  renderWebviewHtml,
  webviewRoot,
} from "./html";
import { handleWebviewMessage } from "./messaging";
import { VSCODE_SHELL, type WebviewHost } from "./types";
import { currentWorkspaceRoot } from "../workspace/roots";

export class TaskViewProvider implements vscode.WebviewViewProvider {
  static readonly viewType = "openaide.tasks";
  private view: vscode.WebviewView | undefined;
  private detachAppServerView: (() => void) | undefined;

  constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly runtime: RuntimeClient,
    private readonly runtimeProcess: RuntimeProcess,
    private readonly logger: ExtensionLogger,
    private readonly surfaces: WebviewHost,
  ) {}

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

  private bootstrap(): Parameters<typeof renderWebviewHtml>[2] {
    return {
      surface: "navigation",
      shell: VSCODE_SHELL,
      projectId: currentWorkspaceRoot()?.projectId,
    };
  }
}
