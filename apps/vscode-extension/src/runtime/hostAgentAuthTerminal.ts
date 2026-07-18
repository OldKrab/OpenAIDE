import * as vscode from "vscode";
import type { RuntimeClient } from "./rpcClient";

const AGENT_AUTH_TERMINAL = "agent/auth_terminal";

type AgentAuthTerminalParams = {
  command?: unknown;
  args?: unknown;
  env?: unknown;
};

/** Opens an ACP-advertised sign-in command in a user-visible terminal. */
export async function openAgentAuthTerminal(params: unknown): Promise<Record<string, never>> {
  const request = params as AgentAuthTerminalParams;
  if (
    typeof request?.command !== "string"
    || request.command.length === 0
    || (request.args !== undefined && (!Array.isArray(request.args) || request.args.some((arg) => typeof arg !== "string")))
    || (request.env !== undefined && !isStringRecord(request.env))
  ) {
    throw new Error("Invalid Agent authentication terminal request.");
  }

  const terminal = vscode.window.createTerminal({
    name: "Agent sign in",
    shellPath: request.command,
    shellArgs: request.args as string[] | undefined,
    env: request.env as Record<string, string> | undefined,
  });
  terminal.show();
  return {};
}

export function registerAgentAuthTerminalHandler(runtime: RuntimeClient): vscode.Disposable {
  return runtime.onHostRequest(AGENT_AUTH_TERMINAL, openAgentAuthTerminal);
}

function isStringRecord(value: unknown): value is Record<string, string> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  return Object.values(value).every((item) => typeof item === "string");
}
