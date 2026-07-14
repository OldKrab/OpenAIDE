import * as vscode from "vscode";
import { randomBytes } from "node:crypto";
import type { WebviewBootstrap } from "./types";

export function renderWebviewHtml(
  context: vscode.ExtensionContext,
  webview: vscode.Webview,
  bootstrap: WebviewBootstrap,
) {
  const root = webviewRoot(context);
  const scriptUri = webview.asWebviewUri(vscode.Uri.joinPath(root, "dist/assets/index.js"));
  const styleUri = webview.asWebviewUri(vscode.Uri.joinPath(root, "dist/assets/index.css"));
  const nonce = cryptoRandom();
  const connectSource = appServerConnectSource(bootstrap);
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource}; font-src ${webview.cspSource}; script-src 'nonce-${nonce}';${connectSource ? ` connect-src ${connectSource};` : ""}">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <link rel="stylesheet" href="${styleUri}">
  <title>OpenAIDE</title>
</head>
<body data-shell="${escapeAttribute(bootstrap.shell.kind)}" data-navigation-mode="${escapeAttribute(bootstrap.shell.navigationMode)}" data-surface="${escapeAttribute(bootstrap.surface)}" data-client-instance-id="${escapeAttribute(bootstrap.clientInstanceId ?? "")}" data-task-id="${escapeAttribute(bootstrap.taskId ?? "")}" data-project-id="${escapeAttribute(bootstrap.projectId ?? "")}" data-composer-submit-shortcut="${escapeAttribute(bootstrap.preferences?.composer_submit_shortcut ?? "enter")}" data-app-server-connection="${escapeAttribute(JSON.stringify(bootstrap.appServerConnection ?? null))}">
  <div id="root"></div>
  <script nonce="${nonce}" type="module" src="${scriptUri}"></script>
</body>
</html>`;
}

export function renderWebviewPreparingHtml(
  context: vscode.ExtensionContext,
  webview: vscode.Webview,
  label = "Preparing OpenAIDE...",
) {
  const root = webviewRoot(context);
  const styleUri = webview.asWebviewUri(vscode.Uri.joinPath(root, "dist/assets/index.css"));
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource}; font-src ${webview.cspSource};">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <link rel="stylesheet" href="${styleUri}">
  <title>OpenAIDE</title>
</head>
<body>
  <main class="app-shell editor-shell">
    <section class="task-surface task-loading" aria-label="OpenAIDE startup">
      <p>${escapeText(label)}</p>
    </section>
  </main>
</body>
</html>`;
}

export function webviewRoot(context: vscode.ExtensionContext) {
  return vscode.Uri.joinPath(context.extensionUri, "webview");
}

export function createWebviewClientInstanceId() {
  return `vscode-webview-${cryptoRandom()}`;
}

function cryptoRandom() {
  return randomBytes(16).toString("hex");
}

function escapeAttribute(value: string) {
  return value.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}

function escapeText(value: string) {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;");
}

function appServerConnectSource(bootstrap: WebviewBootstrap): string | undefined {
  if (bootstrap.appServerConnection?.kind !== "localHttp") return undefined;
  try {
    return new URL(bootstrap.appServerConnection.endpointUrl).origin;
  } catch {
    return undefined;
  }
}
