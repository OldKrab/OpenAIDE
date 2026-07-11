import * as vscode from "vscode";

export function config() {
  return vscode.workspace.getConfiguration("openaide");
}
