import type {
  ComposerMessage,
  TaskId,
  TaskSendIdempotencyKey,
} from "@openaide/app-server-client";
import type { ComposerAttachment } from "../state/composerOptions";

export const PENDING_TASK_SEND_RECOVERY_KEY = "openaide:pending-task-send:v1";

export type PendingTaskSendRecovery = {
  taskId: string;
  taskRevision: number;
  idempotencyKey: TaskSendIdempotencyKey;
  message: ComposerMessage;
  renderState: {
    prompt: string;
    context: ComposerAttachment[];
  };
};

export function savePendingTaskSendRecovery(record: PendingTaskSendRecovery) {
  try {
    globalThis.sessionStorage?.setItem(PENDING_TASK_SEND_RECOVERY_KEY, JSON.stringify(record));
  } catch {
    // Same-client recovery is best effort; send still proceeds without browser storage.
  }
}

export function readPendingTaskSendRecovery(taskId: TaskId | string): PendingTaskSendRecovery | undefined {
  try {
    const raw = globalThis.sessionStorage?.getItem(PENDING_TASK_SEND_RECOVERY_KEY);
    if (!raw) return undefined;
    const parsed = JSON.parse(raw) as Partial<PendingTaskSendRecovery>;
    if (
      parsed.taskId !== taskId ||
      typeof parsed.taskRevision !== "number" ||
      typeof parsed.idempotencyKey !== "string" ||
      !parsed.message ||
      typeof parsed.message !== "object" ||
      !parsed.renderState ||
      typeof parsed.renderState.prompt !== "string" ||
      !Array.isArray(parsed.renderState.context)
    ) {
      return undefined;
    }
    return parsed as PendingTaskSendRecovery;
  } catch {
    return undefined;
  }
}

export function clearPendingTaskSendRecovery(taskId?: TaskId | string) {
  try {
    if (taskId !== undefined) {
      const current = readPendingTaskSendRecovery(taskId);
      if (!current) return;
    }
    globalThis.sessionStorage?.removeItem(PENDING_TASK_SEND_RECOVERY_KEY);
  } catch {
    // Nothing useful to do; the next successful write will replace stale data.
  }
}
