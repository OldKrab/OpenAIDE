import type * as vscode from "vscode";
import type {
  AgentEnvironmentSecretRef,
  HostToWebviewMessage,
  SecretSyncPayload,
  SecretSyncWrite,
  SecretTransactionMessage,
} from "@openaide/app-shell-contracts";
import { customAgentSecretKey } from "../settings/agents";
import { isObject } from "./messagingFields";

type AgentSecretStore = Pick<vscode.SecretStorage, "delete" | "get" | "store">;
type SecretTransactionResult = Extract<HostToWebviewMessage, { type: "secret.transaction.result" }>;
type PendingTransaction = {
  before: Map<string, string | undefined>;
  secretStore: AgentSecretStore;
};

const pendingTransactions = new Map<string, PendingTransaction>();

export async function handleAgentSecretTransaction(
  message: SecretTransactionMessage,
  secretStore: AgentSecretStore | undefined,
): Promise<SecretTransactionResult> {
  const meta = transactionMeta(message.payload);
  try {
    if (!secretStore) throw new Error("Secure storage is unavailable.");
    if (message.type === "secret.transaction.apply") {
      await applyTransaction(meta.transactionId, message.payload.changes, secretStore);
    } else if (message.type === "secret.transaction.commit") {
      commitTransaction(meta.transactionId);
    } else {
      await rollbackTransaction(meta.transactionId);
    }
    return { type: "secret.transaction.result", payload: { ...meta, ok: true } };
  } catch (error) {
    return {
      type: "secret.transaction.result",
      payload: { ...meta, ok: false, error: safeTransactionError(error) },
    };
  }
}

async function applyTransaction(
  transactionId: string,
  value: unknown,
  secretStore: AgentSecretStore,
) {
  if (pendingTransactions.has(transactionId)) throw new Error("Secure storage transaction already exists.");
  const changes = secretSyncPayload(value);
  const targetKeys = changes.writes.map((write) => secretKey(write.target));
  const deleteKeys = changes.deletes.map(secretKey);
  const affectedKeys = [...new Set([...targetKeys, ...deleteKeys])];
  if (affectedKeys.length !== targetKeys.length + deleteKeys.length) {
    throw new Error("Secure storage transaction contains duplicate entries.");
  }

  const before = new Map<string, string | undefined>();
  for (const key of affectedKeys) before.set(key, await secretStore.get(key));
  const resolvedWrites: Array<{ key: string; value: string }> = [];
  for (const write of changes.writes) {
    const value = "value" in write
      ? write.value
      : await secretStore.get(secretKey(write.copyFrom));
    if (value === undefined) throw new Error("A source secret is unavailable.");
    resolvedWrites.push({ key: secretKey(write.target), value });
  }

  try {
    for (const write of resolvedWrites) await secretStore.store(write.key, write.value);
    for (const key of deleteKeys) await secretStore.delete(key);
  } catch {
    await restoreSecrets(secretStore, before);
    throw new Error("Secure storage transaction could not be applied.");
  }
  pendingTransactions.set(transactionId, { before, secretStore });
}

function commitTransaction(transactionId: string) {
  if (!pendingTransactions.delete(transactionId)) {
    throw new Error("Secure storage transaction is unavailable.");
  }
}

async function rollbackTransaction(transactionId: string) {
  const transaction = pendingTransactions.get(transactionId);
  if (!transaction) throw new Error("Secure storage transaction is unavailable.");
  await restoreSecrets(transaction.secretStore, transaction.before);
  pendingTransactions.delete(transactionId);
}

async function restoreSecrets(
  secretStore: AgentSecretStore,
  before: Map<string, string | undefined>,
) {
  for (const [key, value] of before) {
    if (value === undefined) await secretStore.delete(key);
    else await secretStore.store(key, value);
  }
}

function transactionMeta(value: unknown) {
  if (!isObject(value)) throw new Error("Secure storage transaction payload is invalid.");
  const requestId = value.requestId;
  const transactionId = value.transactionId;
  if (typeof requestId !== "string" || !requestId) throw new Error("Secure storage request id is invalid.");
  if (typeof transactionId !== "string" || !transactionId) throw new Error("Secure storage transaction id is invalid.");
  return { requestId, transactionId };
}

function secretSyncPayload(value: unknown): SecretSyncPayload {
  if (!isObject(value)) throw new Error("Secure storage changes are invalid.");
  if (!Array.isArray(value.writes) || !Array.isArray(value.deletes)) {
    throw new Error("Secure storage changes are invalid.");
  }
  return {
    writes: value.writes.map(secretSyncWrite),
    deletes: value.deletes.map(secretRef),
  };
}

function secretSyncWrite(value: unknown): SecretSyncWrite {
  if (!isObject(value)) throw new Error("Secure storage write is invalid.");
  const target = secretRef(value.target);
  const hasValue = typeof value.value === "string";
  const hasCopySource = value.copyFrom !== undefined;
  if (hasValue === hasCopySource) throw new Error("Secure storage write source is invalid.");
  return hasValue
    ? { target, value: value.value as string }
    : { target, copyFrom: secretRef(value.copyFrom) };
}

function secretRef(value: unknown): AgentEnvironmentSecretRef {
  if (!isObject(value) || value.kind !== "agentEnvironment") {
    throw new Error("Secure storage reference is invalid.");
  }
  const agentId = value.agentId;
  const name = value.name;
  if (typeof agentId !== "string" || !/^[A-Za-z0-9_.-]+$/.test(agentId)) {
    throw new Error("Secure storage Agent id is invalid.");
  }
  if (typeof name !== "string" || !/^[_A-Za-z][_A-Za-z0-9]*$/.test(name)) {
    throw new Error("Secure storage environment name is invalid.");
  }
  return { kind: "agentEnvironment", agentId, name };
}

function secretKey(reference: AgentEnvironmentSecretRef) {
  return customAgentSecretKey(reference.agentId, reference.name);
}

function safeTransactionError(error: unknown) {
  if (error instanceof Error && error.message.startsWith("Secure storage")) return error.message;
  if (error instanceof Error && error.message === "A source secret is unavailable.") return error.message;
  return "Secure storage operation failed.";
}
