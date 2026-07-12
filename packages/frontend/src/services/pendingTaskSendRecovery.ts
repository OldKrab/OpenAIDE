import type {
  ClientInstanceId,
  ComposerMessage,
  StateRootId,
  TaskSendIdempotencyKey,
} from "@openaide/app-server-client";
import type { ComposerAttachment } from "../state/composerOptions";

const PENDING_TASK_SEND_RECOVERY_KEY = "openaide:pending-task-send:v3";
const LEGACY_PENDING_TASK_SEND_RECOVERY_KEY = "openaide:pending-task-send:v2";
const RECOVERY_SCHEMA_VERSION = 3;
// Browser policy can block sessionStorage after task/send starts. Keep the
// current process locked on the exact attempt even when reload recovery is unavailable.
const currentProcessRecoveries = new Map<string, PendingTaskSendRecovery>();
// A failed storage removal must not let the same process resurrect an attempt
// it has already settled. A later explicit save replaces this tombstone.
const currentProcessRecoveryTombstones = new Set<string>();

export type PendingTaskSendRecovery = {
  clientInstanceId: ClientInstanceId;
  stateRootId: StateRootId;
  taskId: string;
  taskRevision: number;
  idempotencyKey: TaskSendIdempotencyKey;
  message: ComposerMessage;
  renderState: {
    prompt: string;
    context: ComposerAttachment[];
  };
};

/** Persists one locked send attempt without replacing attempts from other roots, Tasks, or clients. */
export function savePendingTaskSendRecovery(
  record: PendingTaskSendRecovery,
  storage = availableSessionStorage(),
) {
  const key = recoveryStorageKey(record.stateRootId, record.clientInstanceId, record.taskId);
  const processRecord = recoveryRecordForStorage(record);
  currentProcessRecoveries.set(key, processRecord);
  currentProcessRecoveryTombstones.delete(key);
  try {
    discardLegacyRecovery(storage, record.clientInstanceId, record.taskId);
    storage?.setItem(key, JSON.stringify({
      ...processRecord,
      schemaVersion: RECOVERY_SCHEMA_VERSION,
    }));
  } catch {
    // Same-client recovery is best effort; task/send still owns durable idempotency.
  }
}

function recoveryRecordForStorage(record: PendingTaskSendRecovery): PendingTaskSendRecovery {
  return {
    ...record,
    renderState: {
      ...record.renderState,
      // Preview data URLs duplicate uploaded image bytes and can exceed the
      // browser's small session-storage quota. Recovery needs only safe labels
      // and the opaque handles already present in the exact send request.
      context: record.renderState.context.map(({ preview_url: _previewUrl, ...attachment }) => attachment),
    },
  };
}

export function readPendingTaskSendRecovery(
  stateRootId: StateRootId | string,
  clientInstanceId: ClientInstanceId | string,
  taskId: string,
  storage = availableSessionStorage(),
): PendingTaskSendRecovery | undefined {
  const key = recoveryStorageKey(stateRootId, clientInstanceId, taskId);
  const processRecovery = currentProcessRecoveries.get(key);
  if (processRecovery) return processRecovery;
  if (currentProcessRecoveryTombstones.has(key)) return undefined;
  if (!storage) return undefined;
  try {
    discardLegacyRecovery(storage, clientInstanceId, taskId);
    const raw = storage?.getItem(key);
    if (!raw) {
      // A quota failure can reject the write while reads keep working. The
      // process-local record still prevents a second idempotency key in this tab.
      return currentProcessRecoveries.get(key);
    }
    const parsed: unknown = JSON.parse(raw);
    const record = validRecoveryRecord(parsed, stateRootId, clientInstanceId, taskId);
    if (record) {
      currentProcessRecoveries.set(key, record);
      currentProcessRecoveryTombstones.delete(key);
      return record;
    }
    storage?.removeItem(key);
    currentProcessRecoveries.delete(key);
    currentProcessRecoveryTombstones.add(key);
    return undefined;
  } catch {
    return currentProcessRecoveries.get(key);
  }
}

export function clearPendingTaskSendRecovery(
  stateRootId: StateRootId | string,
  clientInstanceId: ClientInstanceId | string,
  taskId: string,
  idempotencyKey?: TaskSendIdempotencyKey,
  storage = availableSessionStorage(),
) {
  const key = recoveryStorageKey(stateRootId, clientInstanceId, taskId);
  if (idempotencyKey !== undefined) {
    const current = readPendingTaskSendRecovery(stateRootId, clientInstanceId, taskId, storage);
    if (!current || current.idempotencyKey !== idempotencyKey) return;
  }
  currentProcessRecoveries.delete(key);
  currentProcessRecoveryTombstones.add(key);
  try {
    discardLegacyRecovery(storage, clientInstanceId, taskId);
    storage?.removeItem(key);
  } catch {
    // A later successful write can replace stale local recovery state.
  }
}

function validRecoveryRecord(
  value: unknown,
  stateRootId: StateRootId | string,
  clientInstanceId: ClientInstanceId | string,
  taskId: string,
): PendingTaskSendRecovery | undefined {
  if (!isRecord(value)) return undefined;
  if (value.schemaVersion !== RECOVERY_SCHEMA_VERSION) return undefined;
  if (
    !isNonEmptyString(value.stateRootId)
    || value.stateRootId !== stateRootId
    || value.clientInstanceId !== clientInstanceId
    || value.taskId !== taskId
  ) return undefined;
  if (!isNonEmptyString(value.idempotencyKey)) return undefined;
  if (!Number.isSafeInteger(value.taskRevision) || (value.taskRevision as number) < 0) return undefined;
  if (!isRecord(value.message) || !isRecord(value.renderState)) return undefined;
  if (typeof value.renderState.prompt !== "string" || !Array.isArray(value.renderState.context)) return undefined;
  if (typeof value.message.text !== "string" || value.message.text !== value.renderState.prompt) return undefined;

  const context = value.renderState.context;
  if (!context.every(isRecoverableAttachment)) return undefined;
  const contextHandles = context.map((attachment) => attachment.app_server_handle_id as string);
  const messageHandles = value.message.attachments;
  if (messageHandles !== undefined && !Array.isArray(messageHandles)) return undefined;
  if (messageHandles !== undefined && !messageHandles.every(isNonEmptyString)) return undefined;
  if (!sameStrings(contextHandles, messageHandles ?? [])) return undefined;

  return {
    clientInstanceId: value.clientInstanceId as ClientInstanceId,
    idempotencyKey: value.idempotencyKey as TaskSendIdempotencyKey,
    message: {
      text: value.message.text,
      ...(messageHandles?.length ? { attachments: messageHandles as ComposerMessage["attachments"] } : {}),
    },
    renderState: {
      prompt: value.renderState.prompt,
      context: context as ComposerAttachment[],
    },
    stateRootId: value.stateRootId as StateRootId,
    taskId: value.taskId,
    taskRevision: value.taskRevision as number,
  };
}

function isRecoverableAttachment(value: unknown): value is ComposerAttachment {
  if (!isRecord(value)) return false;
  if (value.kind !== "file") return false;
  if (!isNonEmptyString(value.label) || !isNonEmptyString(value.local_id)) return false;
  if (!isNonEmptyString(value.app_server_handle_id)) return false;
  if (value.preview_url !== undefined && typeof value.preview_url !== "string") return false;
  if (value.validation_error !== undefined) return false;
  return true;
}

function sameStrings(left: string[], right: string[]) {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

function recoveryStorageKey(
  stateRootId: StateRootId | string,
  clientInstanceId: ClientInstanceId | string,
  taskId: string,
) {
  return `${PENDING_TASK_SEND_RECOVERY_KEY}:${encodeURIComponent(stateRootId)}:${encodeURIComponent(clientInstanceId)}:${encodeURIComponent(taskId)}`;
}

function discardLegacyRecovery(
  storage: Storage | undefined,
  clientInstanceId: ClientInstanceId | string,
  taskId: string,
) {
  try {
    storage?.removeItem(
      `${LEGACY_PENDING_TASK_SEND_RECOVERY_KEY}:${encodeURIComponent(clientInstanceId)}:${encodeURIComponent(taskId)}`,
    );
  } catch {
    // Legacy data is never read; deletion is best-effort quarantine cleanup.
  }
}

function availableSessionStorage(): Storage | undefined {
  try {
    return globalThis.sessionStorage;
  } catch {
    return undefined;
  }
}
