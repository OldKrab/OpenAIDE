import * as vscode from "vscode";
import type { WebviewAppServerConnection } from "@openaide/app-shell-contracts";

/** Resolves App Server addresses into the webview's network context. */
export async function resolveWebviewAppServerConnection(
  connection: WebviewAppServerConnection,
): Promise<WebviewAppServerConnection> {
  if (connection.kind !== "localHttp") return connection;

  // In Remote SSH, the App Server listens on the extension host while the
  // webview runs locally. VS Code owns the tunnel between those two contexts.
  const endpoint = await vscode.env.asExternalUri(vscode.Uri.parse(connection.endpointUrl));
  return {
    ...connection,
    endpointUrl: endpoint.toString(),
  };
}
