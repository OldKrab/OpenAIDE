import * as vscode from "vscode";
import { SUPPORT_RECOVER_STUCK_SESSIONS } from "@openaide/app-server-client";
import { exportSupportDiagnostics } from "../diagnostics/export";
import { RuntimeProcess } from "../runtime/process";
import { RuntimeClient } from "../runtime/rpcClient";
import { TaskEditorManager } from "../webview/editorManager";

export function registerCommands(
  context: vscode.ExtensionContext,
  taskEditors: TaskEditorManager,
  runtimeProcess: RuntimeProcess,
  runtime: RuntimeClient,
) {
  context.subscriptions.push(
    vscode.commands.registerCommand("openaide.newTask", () => {
      taskEditors.openNewTask();
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("openaide.openSettings", () => {
      taskEditors.openSettings();
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("openaide.runtimeHealth", async () => {
      const health = await runtime.health();
      void vscode.window.showInformationMessage(`OpenAIDE App Server: ${health.status}`);
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("openaide.exportDiagnostics", async () => {
      await exportSupportDiagnostics(runtime, runtimeProcess);
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("openaide.recoverStuckSessions", async () => {
      const result = await runtime.appServerRequest(SUPPORT_RECOVER_STUCK_SESSIONS, {});
      const count = result.recoveredTasks.length;
      const suffix = count === 1 ? "session" : "sessions";
      const message =
        count === 0
          ? "OpenAIDE: no stuck sessions found."
          : `OpenAIDE: recovered ${count} stuck ${suffix}.`;
      void vscode.window.showInformationMessage(message);
    }),
  );
}
