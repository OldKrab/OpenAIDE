import * as path from "node:path";
import * as vscode from "vscode";
import type { RuntimeClient } from "./rpcClient";
import { isFileNotFound, validatedWorkspacePath } from "./workspaceBoundary";

const READ_TEXT_FILE = "fs/read_text_file";
const WRITE_TEXT_FILE = "fs/write_text_file";

type ReadTextFileParams = {
  path: string;
  line?: number;
  limit?: number;
};

type WriteTextFileParams = {
  path: string;
  content: string;
};

export function registerFileSystemHostHandlers(runtime: RuntimeClient): vscode.Disposable {
  const read = runtime.onHostRequest(READ_TEXT_FILE, readTextFile);
  const write = runtime.onHostRequest(WRITE_TEXT_FILE, writeTextFile);
  return {
    dispose: () => {
      read.dispose();
      write.dispose();
    },
  };
}

export async function readTextFile(params: unknown) {
  const request = parseReadTextFileParams(params);
  const uri = vscode.Uri.file(await validatedWorkspacePath(request.path, "existing"));
  const document = await vscode.workspace.openTextDocument(uri);
  return {
    content: documentRangeText(document, request.line, request.limit),
  };
}

export async function writeTextFile(params: unknown) {
  const request = parseWriteTextFileParams(params);
  const uri = vscode.Uri.file(await validatedWorkspacePath(request.path, "write-target"));

  if (!(await existingFile(uri))) {
    await vscode.workspace.fs.createDirectory(vscode.Uri.file(path.dirname(uri.fsPath)));
    await vscode.workspace.fs.writeFile(uri, new Uint8Array());
  }

  const document = await vscode.workspace.openTextDocument(uri);
  if (document.isDirty) {
    throw new Error("Cannot write a file with unsaved editor changes.");
  }
  const documentVersion = document.version;
  const edit = new vscode.WorkspaceEdit();
  edit.replace(uri, fullDocumentRange(document), request.content);
  // VS Code version-checks WorkspaceEdit application. This preflight keeps the
  // replacement range tied to the same clean document version handed to it.
  if (document.isDirty || document.version !== documentVersion) {
    throw new Error("The file changed before the Agent edit could be applied.");
  }
  const applied = await vscode.workspace.applyEdit(edit);
  if (!applied) {
    throw new Error("Unable to apply file edit");
  }
  const saved = await document.save();
  if (!saved) {
    throw new Error("Unable to save file edit");
  }
  if (document.isDirty) {
    throw new Error("File changed while the Agent edit was being saved.");
  }

  return {};
}

async function existingFile(uri: vscode.Uri) {
  try {
    await vscode.workspace.fs.stat(uri);
    return true;
  } catch (error) {
    if (isFileNotFound(error)) return false;
    throw error;
  }
}

function parseReadTextFileParams(params: unknown): ReadTextFileParams {
  const object = objectParams(params);
  const filePath = requiredString(object, "path");
  const line = optionalInteger(object, "line");
  const limit = optionalInteger(object, "limit");
  if (line !== undefined && line < 1) {
    throw new Error("line must be 1 or greater");
  }
  if (limit !== undefined && limit < 0) {
    throw new Error("limit must be 0 or greater");
  }
  return { path: filePath, line, limit };
}

function parseWriteTextFileParams(params: unknown): WriteTextFileParams {
  const object = objectParams(params);
  return {
    path: requiredString(object, "path"),
    content: requiredString(object, "content"),
  };
}

function objectParams(params: unknown): Record<string, unknown> {
  if (!params || typeof params !== "object" || Array.isArray(params)) {
    throw new Error("params must be an object");
  }
  return params as Record<string, unknown>;
}

function requiredString(object: Record<string, unknown>, key: string) {
  const value = object[key];
  if (typeof value !== "string") {
    throw new Error(`${key} must be a string`);
  }
  return value;
}

function optionalInteger(object: Record<string, unknown>, key: string) {
  const value = object[key];
  if (value === undefined) return undefined;
  if (!Number.isInteger(value)) {
    throw new Error(`${key} must be an integer`);
  }
  return value as number;
}

function documentRangeText(document: vscode.TextDocument, line?: number, limit?: number) {
  if (line === undefined && limit === undefined) {
    return document.getText();
  }
  if (limit === 0) return "";

  const startLine = (line ?? 1) - 1;
  if (startLine >= document.lineCount) return "";

  const endLineExclusive =
    limit === undefined ? document.lineCount : Math.min(document.lineCount, startLine + limit);
  const start = new vscode.Position(startLine, 0);
  const end =
    endLineExclusive >= document.lineCount
      ? document.lineAt(document.lineCount - 1).rangeIncludingLineBreak.end
      : new vscode.Position(endLineExclusive, 0);

  return document.getText(new vscode.Range(start, end));
}

function fullDocumentRange(document: vscode.TextDocument) {
  const lastLine = Math.max(0, document.lineCount - 1);
  return new vscode.Range(new vscode.Position(0, 0), document.lineAt(lastLine).rangeIncludingLineBreak.end);
}
