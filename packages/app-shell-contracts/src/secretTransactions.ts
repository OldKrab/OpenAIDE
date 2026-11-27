import type {
  HostToWebviewMessage,
  SecretKeysRequestMessage,
  SecretKeysResultMessage,
  SecretRef,
  SecretSyncPayload,
  SecretSyncWrite,
  SecretTransactionMessage,
} from "./webviewTypes.js";

export type AppShellSecretStore = {
  delete(key: string): PromiseLike<void> | void;
  get(key: string): PromiseLike<string | undefined> | string | undefined;
  /** Every stored key id, for a store that can enumerate (names only). Absent when it cannot. */
  keys?(): PromiseLike<string[]> | string[];
  store(key: string, value: string): PromiseLike<void> | void;
};

/**
 * Key ids this shell declares to the App Server so `secret/read` is routed to a client that has
 * the key. A store that can enumerate lists them directly, which covers secrets saved before any
 * index existed. A store that cannot (a native keychain) keeps an index of the ids it saved,
 * and the shell stays an "unknown holder": it may also hold older secrets it cannot list, so the
 * App Server asks it when no client declared the key. Names only, never values.
 */
export const SECRET_KEY_INDEX_KEY = "openaide.secret-key-index";

export async function readSecretKeyIndex(secretStore: AppShellSecretStore | undefined): Promise<string[]> {
  if (!secretStore) return [];
  try {
    const raw = await secretStore.get(SECRET_KEY_INDEX_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? parsed.filter((key): key is string => typeof key === "string") : [];
  } catch {
    return [];
  }
}

/** Answers a `secret.keys.request`; an unreadable store holds no declarable keys. */
export async function secretKeysResult(
  message: SecretKeysRequestMessage,
  secretStore: AppShellSecretStore | undefined,
): Promise<SecretKeysResultMessage> {
  const requestId = message.payload.requestId;
  if (secretStore?.keys) {
    try {
      const keyIds = (await secretStore.keys()).filter((key) => key !== SECRET_KEY_INDEX_KEY);
      return { type: "secret.keys.result", payload: { requestId, keyIds, unknownHolder: false } };
    } catch {
      // Fall through: an unlistable store may still hold keys, so the shell stays an unknown holder.
      return { type: "secret.keys.result", payload: { requestId, keyIds: [], unknownHolder: true } };
    }
  }
  return {
    type: "secret.keys.result",
    payload: { requestId, keyIds: await readSecretKeyIndex(secretStore), unknownHolder: Boolean(secretStore) },
  };
}

type SecretTransactionResult = Extract<HostToWebviewMessage, { type: "secret.transaction.result" }>;
type PendingTransaction = {
  before: Map<string, string | undefined>;
  secretStore: AppShellSecretStore;
};

/** Validates and executes the shared apply/commit/rollback contract over a shell-native store. */
export function createSecretTransactionHandler() {
  const pendingTransactions = new Map<string, PendingTransaction>();

  return async function handleSecretTransaction(
    message: SecretTransactionMessage,
    secretStore: AppShellSecretStore | undefined,
  ): Promise<SecretTransactionResult> {
    const meta = transactionMeta(message.payload);
    try {
      if (!secretStore) throw new Error("Secure storage is unavailable.");
      if (message.type === "secret.transaction.apply") {
        await applyTransaction(meta.transactionId, message.payload.changes, secretStore, pendingTransactions);
      } else if (message.type === "secret.transaction.commit") {
        commitTransaction(meta.transactionId, pendingTransactions);
      } else {
        await rollbackTransaction(meta.transactionId, pendingTransactions);
      }
      return { type: "secret.transaction.result", payload: { ...meta, ok: true } };
    } catch (error) {
      return {
        type: "secret.transaction.result",
        payload: { ...meta, ok: false, error: safeTransactionError(error) },
      };
    }
  };
}

async function applyTransaction(
  transactionId: string,
  value: unknown,
  secretStore: AppShellSecretStore,
  pendingTransactions: Map<string, PendingTransaction>,
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
  for (const key of secretStore.keys ? affectedKeys : [...affectedKeys, SECRET_KEY_INDEX_KEY]) before.set(key, await secretStore.get(key));
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
    if (!secretStore.keys) {
      const index = new Set(await readSecretKeyIndex(secretStore));
      for (const write of resolvedWrites) index.add(write.key);
      for (const key of deleteKeys) index.delete(key);
      await secretStore.store(SECRET_KEY_INDEX_KEY, JSON.stringify([...index].sort()));
    }
  } catch {
    await restoreSecrets(secretStore, before);
    throw new Error("Secure storage transaction could not be applied.");
  }
  pendingTransactions.set(transactionId, { before, secretStore });
}

function commitTransaction(
  transactionId: string,
  pendingTransactions: Map<string, PendingTransaction>,
) {
  if (!pendingTransactions.delete(transactionId)) {
    throw new Error("Secure storage transaction is unavailable.");
  }
}

async function rollbackTransaction(
  transactionId: string,
  pendingTransactions: Map<string, PendingTransaction>,
) {
  const transaction = pendingTransactions.get(transactionId);
  if (!transaction) throw new Error("Secure storage transaction is unavailable.");
  await restoreSecrets(transaction.secretStore, transaction.before);
  pendingTransactions.delete(transactionId);
}

async function restoreSecrets(
  secretStore: AppShellSecretStore,
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

function secretRef(value: unknown): SecretRef {
  if (!isObject(value)) throw new Error("Secure storage reference is invalid.");
  if (value.kind === "agentEnvironment") {
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
  if (value.kind === "mcp") {
    const serverId = value.serverId;
    const field = value.field;
    const name = value.name;
    if (typeof serverId !== "string" || !/^[A-Za-z0-9_.-]+$/.test(serverId)) {
      throw new Error("Secure storage MCP server id is invalid.");
    }
    if (field !== "env" && field !== "header") throw new Error("Secure storage MCP field kind is invalid.");
    if (typeof name !== "string") throw new Error("Secure storage MCP field name is invalid.");
    const validName = field === "env"
      ? /^[_A-Za-z][_A-Za-z0-9]*$/.test(name)
      : /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/.test(name);
    if (!validName) throw new Error("Secure storage MCP field name is invalid.");
    return { kind: "mcp", serverId, field, name };
  }
  throw new Error("Secure storage reference is invalid.");
}

export function secretStorageKey(reference: SecretRef) {
  return reference.kind === "agentEnvironment"
    ? `openaide.agent.${reference.agentId}.env.${reference.name}`
    : `openaide.mcp.${reference.serverId}.${reference.field}.${reference.name}`;
}

function secretKey(reference: SecretRef) {
  return secretStorageKey(reference);
}

function safeTransactionError(error: unknown) {
  if (error instanceof Error && error.message.startsWith("Secure storage")) return error.message;
  if (error instanceof Error && error.message === "A source secret is unavailable.") return error.message;
  return "Secure storage operation failed.";
}

function isObject(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
