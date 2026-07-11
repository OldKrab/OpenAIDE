import * as vscode from "vscode";
import type { RuntimeClient } from "./rpcClient";
import { customAgentSecretKey } from "../settings/agents";

const AGENT_SECRET_ENV = "agent/secret_env";

type SecretEnvParams = {
  agent_id?: unknown;
  names?: unknown;
};

export function registerAgentSecretHandlers(runtime: RuntimeClient, secrets: vscode.SecretStorage): vscode.Disposable {
  return runtime.onHostRequest(AGENT_SECRET_ENV, async (params) => secretEnv(params, secrets));
}

export async function secretEnv(params: unknown, secrets: Pick<vscode.SecretStorage, "get">) {
  const request = params as SecretEnvParams;
  if (typeof request?.agent_id !== "string" || !Array.isArray(request.names)) {
    throw new Error("Invalid Agent secret request.");
  }
  const env: Record<string, string> = {};
  for (const name of request.names) {
    if (typeof name !== "string") continue;
    const value = await secrets.get(customAgentSecretKey(request.agent_id, name));
    if (value === undefined) {
      throw new Error(`Missing secret environment value: ${name}`);
    }
    env[name] = value;
  }
  return { env };
}
