import type * as vscode from "vscode";
import {
  createSecretTransactionHandler,
  secretKeysResult,
  type SecretKeysRequestMessage,
  type AppShellSecretStore,
  type SecretTransactionMessage,
} from "@openaide/app-shell-contracts";

type AgentSecretStore = Pick<vscode.SecretStorage, "delete" | "get" | "store"> & AppShellSecretStore;
const handleSecretTransaction = createSecretTransactionHandler();

export async function handleAgentSecretTransaction(
  message: SecretTransactionMessage,
  secretStore: AgentSecretStore | undefined,
): ReturnType<ReturnType<typeof createSecretTransactionHandler>> {
  return handleSecretTransaction(message, secretStore);
}

export function handleAgentSecretKeys(
  message: SecretKeysRequestMessage,
  secretStore: AgentSecretStore | undefined,
) {
  return secretKeysResult(message, secretStore);
}
