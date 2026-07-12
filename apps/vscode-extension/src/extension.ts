import * as vscode from "vscode";
import { registerCommands } from "./commands";
import { ExtensionLogger } from "./logging/logger";
import { RuntimeProcess } from "./runtime/process";
import { registerFileSystemHostHandlers } from "./runtime/hostFileSystem";
import { registerAgentSecretHandlers } from "./runtime/hostAgentSecrets";
import { registerTerminalHostHandlers } from "./runtime/hostTerminal";
import { RuntimeClient } from "./runtime/rpcClient";
import { TaskEditorManager } from "./webview/editorManager";
import { TaskViewProvider } from "./webview/navigationProvider";
import { registerWorkspaceProjectSync } from "./workspace/projectSync";

export async function activate(context: vscode.ExtensionContext) {
  const logger = new ExtensionLogger("openaide");
  const runtimeProcess = new RuntimeProcess(context, logger);
  const runtime = new RuntimeClient(runtimeProcess, logger);
  const taskEditors = new TaskEditorManager(context, runtime, runtimeProcess, logger);
  const fileSystemHostHandlers = registerFileSystemHostHandlers(runtime);
  const agentSecretHandlers = registerAgentSecretHandlers(runtime, context.secrets);
  const terminalHostHandlers = registerTerminalHostHandlers(runtime);
  const workspaceProjectSync = registerWorkspaceProjectSync(runtime, logger);
  await workspaceProjectSync.ready;
  const taskViewProvider = new TaskViewProvider(context, runtime, runtimeProcess, logger, taskEditors);

  context.subscriptions.push(runtime);
  context.subscriptions.push(runtimeProcess);
  context.subscriptions.push(fileSystemHostHandlers);
  context.subscriptions.push(agentSecretHandlers);
  context.subscriptions.push(terminalHostHandlers);
  context.subscriptions.push(workspaceProjectSync);
  context.subscriptions.push(taskEditors);
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(TaskViewProvider.viewType, taskViewProvider, {
      webviewOptions: { retainContextWhenHidden: true },
    }),
  );

  registerCommands(context, taskEditors, runtimeProcess, runtime);
}

export async function deactivate() {
  // Disposables registered in activate own runtime shutdown order.
}
