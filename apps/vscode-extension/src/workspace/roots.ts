import * as vscode from "vscode";
import { projectIdForWorkspaceRoot } from "@openaide/app-shell-contracts";

export type WorkspaceRoot = {
  path: string;
  label: string;
  projectId: string;
};

export function firstWorkspaceRoot() {
  return vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? "";
}

export function workspaceRoots(): WorkspaceRoot[] {
  const current = currentWorkspaceFolder();
  const folders = vscode.workspace.workspaceFolders ?? [];
  const ordered = current ? [current, ...folders.filter((folder) => folder !== current)] : folders;
  return ordered.map((folder) => ({
    path: folder.uri.fsPath,
    label: folder.name,
    projectId: projectIdForWorkspaceRoot(folder.uri.fsPath),
  }));
}

export function currentWorkspaceRoot(): WorkspaceRoot | undefined {
  return workspaceRoots()[0];
}

function currentWorkspaceFolder() {
  const activeUri = vscode.window.activeTextEditor?.document.uri;
  return activeUri ? vscode.workspace.getWorkspaceFolder(activeUri) : vscode.workspace.workspaceFolders?.[0];
}
