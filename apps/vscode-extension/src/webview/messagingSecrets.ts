import type * as vscode from "vscode";
import {
  createSecretTransactionHandler,
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
