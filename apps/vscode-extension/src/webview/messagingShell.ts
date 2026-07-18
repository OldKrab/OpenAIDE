import * as nodePath from "node:path";
import * as vscode from "vscode";
import {
  SECRET_READ,
  SHELL_REVEAL_FILE,
  SHELL_RESOLVE_FILE_REVEAL,
  SHELL_SHOW_NOTIFICATION,
  WORKTREE_RESOLVE_FOLDER,
  type SecretReadParams,
  type ShellNotificationAction,
  type ShellNotificationLevel,
  type ShellRevealFileParams,
  type ShellShowNotificationParams,
  type WorktreeId,
  type WorktreeRepositoryId,
} from "@openaide/app-server-client";
import type { WebviewToHostMessage } from "@openaide/app-shell-contracts";
import { validatedWorkspacePath } from "../runtime/workspaceBoundary";
import { workspaceRoots } from "../workspace/roots";
import type { MessageContext } from "./messagingContext";
import { isObject } from "./messagingFields";
import { handleAgentSecretTransaction } from "./messagingSecrets";

export async function routeSurfaceCommand(message: WebviewToHostMessage, context: MessageContext) {
  if (message.type === "surface.openNewTask") {
    context.surfaces?.openNewTask(message.payload?.project_id);
    return true;
  }
  if (message.type === "surface.openSettings") {
    context.surfaces?.openSettings();
    return true;
  }
  if (message.type === "surface.openTask" && isObject(message.payload)) {
    const taskId = typeof message.payload.task_id === "string" ? message.payload.task_id : undefined;
    if (taskId) {
      const title = typeof message.payload.title === "string" ? message.payload.title : undefined;
      context.adoptTask?.(taskId, title);
      context.surfaces?.openTask(taskId, title);
    }
    return true;
  }
  return false;
}

export async function routeHostCapabilityCommand(message: WebviewToHostMessage, context: MessageContext) {
  if (
    message.type === "secret.transaction.apply" ||
    message.type === "secret.transaction.commit" ||
    message.type === "secret.transaction.rollback"
  ) {
    await context.post(await handleAgentSecretTransaction(message, context.agentSecretStore));
    return true;
  }
  if (message.type === "appServer.serverRequest") {
    await routeAppServerServerRequest(message.payload, context);
    return true;
  }
  if (message.type === "workspace.roots") {
    await context.post({ type: "workspace.roots.result", payload: { roots: workspaceRoots() } });
    return true;
  }
  if (message.type === "worktree.openFolder" && isObject(message.payload)) {
    const repositoryId = requiredString(message.payload, "repository_id") as WorktreeRepositoryId;
    const worktreeId = requiredString(message.payload, "worktree_id") as WorktreeId;
    const result = await context.runtime.appServerRequest(WORKTREE_RESOLVE_FOLDER, {
      repositoryId,
      worktreeId,
    });
    const folder = vscode.Uri.file(result.path);
    const inWorkspace = vscode.workspace.workspaceFolders?.some(({ uri }) => pathContains(uri.fsPath, result.path)) ?? false;
    await vscode.commands.executeCommand(inWorkspace ? "revealInExplorer" : "revealFileInOS", folder);
    return true;
  }
  if (message.type === "tool.openPath" && isObject(message.payload)) {
    const path = typeof message.payload.path === "string" ? message.payload.path : "";
    const line = typeof message.payload.line === "number" && message.payload.line > 0 ? message.payload.line : undefined;
    const document = await vscode.workspace.openTextDocument(vscode.Uri.file(await validatedWorkspacePath(path, "existing")));
    await vscode.window.showTextDocument(document, {
      preview: true,
      selection: line ? new vscode.Range(line - 1, 0, line - 1, 0) : undefined,
    });
    return true;
  }
  return false;
}

function pathContains(root: string, candidate: string) {
  const relative = nodePath.relative(root, candidate);
  return relative === "" || (!relative.startsWith(`..${nodePath.sep}`) && relative !== ".." && !nodePath.isAbsolute(relative));
}

async function routeAppServerServerRequest(
  payload: Extract<WebviewToHostMessage, { type: "appServer.serverRequest" }>["payload"],
  context: MessageContext,
) {
  const result = await appServerServerRequestResult(payload.method, payload.params, context);
  await context.post({
    type: "appServer.serverRequest.result",
    payload: {
      requestId: payload.requestId,
      method: payload.method,
      result,
    },
  });
}

async function appServerServerRequestResult(
  method: string,
  params: unknown,
  context: MessageContext,
) {
  if (method === SECRET_READ) {
    const request = secretReadParams(params);
    return {
      value: context.agentSecretStore ? await context.agentSecretStore.get(request.key) : null,
    };
  }
  if (method === SHELL_SHOW_NOTIFICATION) {
    const request = shellShowNotificationParams(params);
    return {
      actionId: await showShellNotification(request),
    };
  }
  if (method === SHELL_REVEAL_FILE) {
    const request = shellRevealFileParams(params);
    return {
      revealed: await revealShellFile(request, context),
    };
  }
  throw new Error(`Unsupported App Server request method: ${method}`);
}

function secretReadParams(params: unknown): SecretReadParams {
  const object = objectParams(params);
  return {
    key: requiredString(object, "key"),
    label: optionalString(object, "label"),
  };
}

function shellShowNotificationParams(params: unknown): ShellShowNotificationParams {
  const object = objectParams(params);
  const level = notificationLevel(object.level);
  return {
    level,
    message: requiredString(object, "message"),
    actions: notificationActions(object.actions),
  };
}

function shellRevealFileParams(params: unknown): ShellRevealFileParams {
  const object = objectParams(params);
  return {
    originatingClientInstanceId: requiredString(
      object,
      "originatingClientInstanceId",
    ) as ShellRevealFileParams["originatingClientInstanceId"],
    fileHandleId: requiredString(object, "fileHandleId"),
    label: optionalString(object, "label"),
  };
}

async function revealShellFile(request: ShellRevealFileParams, context: MessageContext) {
  let target: { path: string; label: string } | undefined;
  try {
    target = await context.runtime.appServerRequest(SHELL_RESOLVE_FILE_REVEAL, {
      originatingClientInstanceId: request.originatingClientInstanceId,
      fileHandleId: request.fileHandleId,
    });
  } catch {
    return false;
  }
  const document = await vscode.workspace.openTextDocument(
    vscode.Uri.file(await validatedWorkspacePath(target.path, "existing")),
  );
  await vscode.window.showTextDocument(document, { preview: true });
  return true;
}

async function showShellNotification(request: ShellShowNotificationParams) {
  const labels = (request.actions ?? []).map((action) => action.label);
  const selected = await showNotificationForLevel(request.level, request.message, labels);
  return (request.actions ?? []).find((action) => action.label === selected)?.actionId ?? null;
}

function showNotificationForLevel(
  level: ShellNotificationLevel,
  message: string,
  labels: string[],
) {
  if (level === "error") return vscode.window.showErrorMessage(message, ...labels);
  if (level === "warning") return vscode.window.showWarningMessage(message, ...labels);
  return vscode.window.showInformationMessage(message, ...labels);
}

function notificationLevel(value: unknown): ShellNotificationLevel {
  if (value === "info" || value === "warning" || value === "error") return value;
  throw new Error("level must be a shell notification level");
}

function notificationActions(value: unknown): ShellNotificationAction[] {
  if (value === undefined) return [];
  if (!Array.isArray(value)) throw new Error("actions must be an array");
  return value.map((item) => {
    const object = objectParams(item);
    return {
      actionId: requiredString(object, "actionId"),
      label: requiredString(object, "label"),
    };
  });
}

function objectParams(params: unknown): Record<string, unknown> {
  if (!isObject(params)) throw new Error("params must be an object");
  return params;
}

function requiredString(object: Record<string, unknown>, key: string) {
  const value = object[key];
  if (typeof value !== "string") throw new Error(`${key} must be a string`);
  return value;
}

function optionalString(object: Record<string, unknown>, key: string) {
  const value = object[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string") throw new Error(`${key} must be a string`);
  return value;
}
