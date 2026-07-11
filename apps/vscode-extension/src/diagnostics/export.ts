import * as vscode from "vscode";
import { RuntimeProcess } from "../runtime/process";
import { RuntimeClient } from "../runtime/rpcClient";
import { collectDiagnostics } from "./snapshot";

export async function openDiagnosticsDocument(runtime: RuntimeClient, runtimeProcess: RuntimeProcess) {
  const snapshot = await collectDiagnostics(runtime, runtimeProcess);
  const document = await vscode.workspace.openTextDocument({
    content: JSON.stringify(snapshot, null, 2),
    language: "json",
  });
  await vscode.window.showTextDocument(document, { preview: true });
}
