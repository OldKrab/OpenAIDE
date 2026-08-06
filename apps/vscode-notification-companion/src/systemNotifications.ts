import { execFile } from "node:child_process";

const APPLICATION_NAME = "OpenAIDE";
// The stable VS Code installer registers this AUMID; failures (including portable
// installs without the shortcut) are surfaced so the caller can use its workbench fallback.
const VSCODE_WINDOWS_APP_ID = "Microsoft.VisualStudioCode";

export type NotificationProcessLauncher = (
  executable: string,
  args: string[],
) => Promise<void>;

/** Creates a shell-free launcher for the current platform's OS notification service. */
export function createSystemNotificationSender(
  platform: NodeJS.Platform,
  launch: NotificationProcessLauncher = launchProcess,
) {
  return async (message: string): Promise<void> => {
    if (platform === "linux") {
      await launch("notify-send", [
        `--app-name=${APPLICATION_NAME}`,
        APPLICATION_NAME,
        message,
      ]);
      return;
    }
    if (platform === "darwin") {
      await launch("osascript", [
        "-e",
        "on run argv",
        "-e",
        "display notification (item 2 of argv) with title (item 1 of argv)",
        "-e",
        "end run",
        "--",
        APPLICATION_NAME,
        message,
      ]);
      return;
    }
    if (platform === "win32") {
      await launch("powershell.exe", [
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        windowsNotificationCommand(message),
      ]);
      return;
    }
    throw new Error(`OS notifications are not supported on ${platform}`);
  };
}

function windowsNotificationCommand(message: string) {
  const encodedMessage = Buffer.from(message, "utf8").toString("base64");
  return [
    `$message=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('${encodedMessage}'))`,
    "[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime]>$null",
    "$template=[Windows.UI.Notifications.ToastTemplateType]::ToastText02",
    "$xml=[Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent($template)",
    "$text=$xml.GetElementsByTagName('text')",
    `$text.Item(0).AppendChild($xml.CreateTextNode('${APPLICATION_NAME}'))>$null`,
    "$text.Item(1).AppendChild($xml.CreateTextNode($message))>$null",
    "$toast=[Windows.UI.Notifications.ToastNotification]::new($xml)",
    `[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('${VSCODE_WINDOWS_APP_ID}').Show($toast)`,
  ].join(";");
}

function launchProcess(executable: string, args: string[]) {
  return new Promise<void>((resolve, reject) => {
    execFile(executable, args, { windowsHide: true }, (error) => {
      if (error) reject(error);
      else resolve();
    });
  });
}
