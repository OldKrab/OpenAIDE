import * as vscode from "vscode";
import { registerCommands } from "./commands";
import { ExtensionLogger } from "./logging/logger";
import { RuntimeProcess } from "./runtime/process";
import { registerFileSystemHostHandlers } from "./runtime/hostFileSystem";
import { registerAgentSecretHandlers } from "./runtime/hostAgentSecrets";
import { registerAgentAuthTerminalHandler } from "./runtime/hostAgentAuthTerminal";
import { registerTerminalHostHandlers } from "./runtime/hostTerminal";
import { RuntimeClient } from "./runtime/rpcClient";
import { registerTaskNotifications } from "./notifications/taskNotifications";
import { TaskEditorManager } from "./webview/editorManager";
import { TaskViewProvider } from "./webview/navigationProvider";
import { registerWorkspaceProjectSync } from "./workspace/projectSync";

let activeRuntime: RuntimeClient | undefined;

export async function activate(context: vscode.ExtensionContext) {
  const logger = new ExtensionLogger("openaide");
  const runtimeProcess = new RuntimeProcess(context, logger);
  const runtime = new RuntimeClient(runtimeProcess, logger);
  activeRuntime = runtime;
  const taskEditors = new TaskEditorManager(context, runtime, runtimeProcess, logger);
  const taskViewProvider = new TaskViewProvider(context, runtime, runtimeProcess, logger, taskEditors);
  const fileSystemHostHandlers = registerFileSystemHostHandlers(runtime);
  const agentSecretHandlers = registerAgentSecretHandlers(runtime, context.secrets);
  const agentAuthTerminalHandler = registerAgentAuthTerminalHandler(runtime);
  const terminalHostHandlers = registerTerminalHostHandlers(runtime);
  const workspaceProjectSync = registerWorkspaceProjectSync(runtime, logger, (roots) => {
    taskEditors.updateWorkspaceRoots(roots);
    taskViewProvider.updateWorkspaceRoots(roots);
  });
  context.subscriptions.push(runtime);
  context.subscriptions.push(runtimeProcess);
  context.subscriptions.push(fileSystemHostHandlers);
  context.subscriptions.push(agentSecretHandlers);
  context.subscriptions.push(agentAuthTerminalHandler);
  context.subscriptions.push(terminalHostHandlers);
  context.subscriptions.push(workspaceProjectSync);
  context.subscriptions.push(taskEditors);
  context.subscriptions.push(taskViewProvider);
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(TaskViewProvider.viewType, taskViewProvider, {
      webviewOptions: { retainContextWhenHidden: true },
    }),
  );

  registerCommands(context, taskEditors, runtimeProcess, runtime);

  // Activation owns registration only. App Server consumers share the same
  // in-flight connection attempt and settle independently in the background so
  // VS Code can render the Tasks view and its recoverable unavailable state.
  void workspaceProjectSync.ready;
  void registerTaskNotifications(
    runtime,
    context.globalState,
    taskEditors,
    logger,
  ).then((taskNotifications) => {
    if (activeRuntime !== runtime) {
      taskNotifications.dispose();
      return;
    }
    context.subscriptions.push(taskNotifications);
  }, () => {
    logger.warn("failed to register VS Code Task notifications", {
      error_kind: "task_notification_registration_failed",
    });
  });
}

export async function deactivate() {
  const runtime = activeRuntime;
  activeRuntime = undefined;
  await runtime?.close();
}
