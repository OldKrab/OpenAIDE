import * as vscode from "vscode";
import { registerNotificationPresenter } from "./notificationPresenter";
import { createSystemNotificationSender } from "./systemNotifications";

/** Activates on the local UI host and owns all operating-system integration. */
export function activate(context: vscode.ExtensionContext) {
  const output = vscode.window.createOutputChannel("OpenAIDE Notifications");
  const notify = createSystemNotificationSender(process.platform);
  const presenter = registerNotificationPresenter({
    registerCommand: (command, handler) => vscode.commands.registerCommand(command, handler),
    notify,
    reportFailure: (message) => {
      void vscode.window.showErrorMessage(message);
    },
    log: (message, fields) => {
      const details = fields ? ` ${JSON.stringify(fields)}` : "";
      output.appendLine(`${new Date().toISOString()} ${message}${details}`);
    },
  });

  context.subscriptions.push(output, presenter);
}
