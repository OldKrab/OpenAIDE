import {
  TASK_DISCARD,
  type BackendConnection,
  type TaskId,
  type TaskSendIdempotencyKey,
} from "@openaide/app-server-client";
import type { ComposerAttachmentResourceOwner } from "../services/attachmentResources";
import type { AppAction } from "../state/appReducer";
import type { AppState } from "../state/store";

export type PreparedTaskLease = Readonly<{
  generation: number;
  preparationKey: string;
  taskId: TaskId;
}>;

type PreparedTaskDisposal = {
  attachmentResources?: ComposerAttachmentResourceOwner;
  dispatch: (action: AppAction) => void;
  lease?: PreparedTaskLease;
  request?: NonNullable<BackendConnection["request"]>;
  taskId: TaskId;
};

/** Owns the one empty Task behind New Task until it is sent or explicitly discarded. */
export class PreparedTaskOwnership {
  private current?: PreparedTaskLease;
  private generation = 0;
  // Settled IDs stay for this controller's lifetime: late preparation/browser
  // promises can otherwise issue a second discard after the first one completes.
  private readonly disposals = new Map<TaskId, Promise<void>>();
  // Send protection is independent of the current prepared lease. A newer New Task
  // must not make an older in-flight or ambiguous send disposable.
  private readonly sendProtections = new Map<string, TaskId>();

  claim({
    attachmentResources,
    preparationKey,
    taskId,
  }: {
    attachmentResources?: ComposerAttachmentResourceOwner;
    preparationKey: string;
    taskId: TaskId;
  }): PreparedTaskLease {
    if (this.current?.taskId === taskId && this.current.preparationKey === preparationKey) {
      attachmentResources?.claimPreparedTask(taskId);
      return this.current;
    }
    const lease = { generation: ++this.generation, preparationKey, taskId };
    this.current = lease;
    attachmentResources?.claimPreparedTask(taskId);
    return lease;
  }

  currentLease(taskId?: TaskId | string) {
    if (taskId !== undefined && this.current?.taskId !== taskId) return undefined;
    return this.current;
  }

  currentTaskId() {
    return this.current?.taskId;
  }

  ownsPreparation(preparationKey: string) {
    return this.current?.preparationKey === preparationKey;
  }

  isCurrent(lease: PreparedTaskLease) {
    return this.current?.generation === lease.generation;
  }

  /** Protects a send independently from subsequent New Task lease changes. */
  protectSend(lease: PreparedTaskLease, idempotencyKey: TaskSendIdempotencyKey | string) {
    if (this.isCurrent(lease)) this.current = undefined;
    this.sendProtections.set(idempotencyKey, lease.taskId);
  }

  settleSend(idempotencyKey: TaskSendIdempotencyKey | string) {
    this.sendProtections.delete(idempotencyKey);
  }

  settleTaskSends(taskId: TaskId | string) {
    for (const [key, protectedTaskId] of this.sendProtections) {
      if (protectedTaskId === taskId) this.sendProtections.delete(key);
    }
  }

  confirmSentTask(taskId: TaskId | string) {
    if (this.current?.taskId === taskId) this.current = undefined;
    this.settleTaskSends(taskId);
  }

  /** Reclaims only the exact lease that started a rejected send. */
  reclaim(
    lease: PreparedTaskLease,
    attachmentResources?: ComposerAttachmentResourceOwner,
  ) {
    if (this.generation !== lease.generation || this.current) return false;
    this.current = lease;
    attachmentResources?.claimPreparedTask(lease.taskId);
    return true;
  }

  /** Records disposal completed by the first-send cancellation workflow. */
  recordDiscarded(taskId: TaskId) {
    if (this.current?.taskId === taskId) this.current = undefined;
    this.settleTaskSends(taskId);
    if (!this.disposals.has(taskId)) this.disposals.set(taskId, Promise.resolve());
  }

  isDisposable(taskId: TaskId | string) {
    const sendProtected = [...this.sendProtections.values()].some((protectedTaskId) => (
      protectedTaskId === taskId
    ));
    return !sendProtected && !this.disposals.has(taskId as TaskId);
  }

  /** Drops identities that are not comparable after the state root changes. */
  replaceStateRoot() {
    this.generation += 1;
    this.current = undefined;
    this.disposals.clear();
    this.sendProtections.clear();
  }

  /** Discards an empty Task at most once and only for its current lease. */
  discard({ attachmentResources, dispatch, lease, request, taskId }: PreparedTaskDisposal) {
    if (!this.isDisposable(taskId)) return Promise.resolve();
    if (lease && !this.isCurrent(lease)) return Promise.resolve();
    const existing = this.disposals.get(taskId);
    if (existing) return existing;
    if (this.current?.taskId === taskId) this.current = undefined;
    attachmentResources?.releaseTask(taskId);
    // Local ownership ends synchronously; backend acknowledgement may settle after routing.
    dispatch({ type: "taskInput:clear", taskId });
    dispatch({ type: "task:list:remove", taskId });
    const disposal = (async () => {
      try {
        await request?.(TASK_DISCARD, { taskId });
      } catch {
        // Navigation and context replacement must still forget a stale local Draft.
      }
    })();
    this.disposals.set(taskId, disposal);
    return disposal;
  }
}

/** Returns only a genuinely disposable New Task snapshot, never a protected send. */
export function disposablePreparedTaskId(
  state: AppState,
  ownership?: PreparedTaskOwnership,
): TaskId | undefined {
  if (state.newTask.submitting || state.snapshot?.task.has_messages !== false) return undefined;
  const taskId = state.snapshot.task.task_id as TaskId;
  return ownership?.isDisposable(taskId) === false ? undefined : taskId;
}
