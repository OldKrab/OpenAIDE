import * as vscode from "vscode";

export type WorkspaceRoot = {
  path: string;
  label: string;
};

export function firstWorkspaceRoot() {
  return vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? "";
}

export function workspaceRoots(): WorkspaceRoot[] {
  return (vscode.workspace.workspaceFolders ?? []).map((folder) => ({
    path: folder.uri.fsPath,
    label: folder.name,
  }));
}
