import {
  TASK_RELEASE,
  type BackendConnection,
  type TaskId,
} from "@openaide/app-server-client";
import type { ComposerAttachmentResourceOwner } from "../services/attachmentResources";
import type { AppAction } from "../state/appReducer";
import type { AppState } from "../state/store";
import type { ConfigOptionsCatalog, TaskSnapshot } from "@openaide/app-shell-contracts";

export type NewTaskLease = Readonly<{
  generation: number;
  preparationKey: string;
  taskId: TaskId;
}>;

type NewTaskDisposal = {
  attachmentResources?: ComposerAttachmentResourceOwner;
  dispatch: (action: AppAction) => void;
  lease?: NewTaskLease;
  request?: NonNullable<BackendConnection["request"]>;
  taskId: TaskId;
};

/** Owns the one client-private New Task from creation until Send or explicit discard. */
export class NewTaskController {
  private current?: NewTaskLease;
  private cachedSnapshot?: TaskSnapshot;
  private settledConfigOptions?: ConfigOptionsCatalog;
  private expiredLeaseTaskId?: TaskId;
  private replacementRequired?: { taskId: TaskId; revision: number };
  private generation = 0;
  private preparationReset = 0;
  private readonly listeners = new Set<() => void>();
  // Settled IDs stay for this controller's lifetime: late creation/browser
  // promises can otherwise issue a second discard after the first one completes.
  private readonly disposals = new Map<TaskId, Promise<void>>();
  // Send protection is independent of the current controller lease. A newer New Task
  // must not make an older in-flight Send disposable before its one request settles.
  private readonly sendProtections = new Set<TaskId>();

  getSnapshot = () => this.cachedSnapshot;

  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  /** Installs a newer snapshot only for this controller's current New Task identity. */
  updateSnapshot(snapshot: TaskSnapshot) {
    if (snapshot.lifecycle !== "prepared") return false;
    if (!this.current && !this.cachedSnapshot && this.expiredLeaseTaskId === snapshot.task.task_id) {
      return false;
    }
    const taskId = this.cachedSnapshot?.task.task_id ?? this.current?.taskId;
    if (taskId !== undefined && taskId !== snapshot.task.task_id) return false;
    if (
      snapshot.preparation?.kind === "failed"
      && snapshot.preparation.recovery === "replaceTask"
    ) {
      const taskId = snapshot.task.task_id as TaskId;
      if (!this.replacementRequired || snapshot.revision > this.replacementRequired.revision) {
        this.replacementRequired = { taskId, revision: snapshot.revision };
        this.cachedSnapshot = snapshot;
        this.emit();
      }
      return true;
    }
    if (
      this.replacementRequired?.taskId === snapshot.task.task_id
      && snapshot.revision <= this.replacementRequired.revision
    ) return true;
    if (this.replacementRequired?.taskId === snapshot.task.task_id) {
      this.replacementRequired = undefined;
    }
    // Async preparation, subscriptions, and attachment work can settle out of
    // order. A delayed acquire result must never roll a ready composer back to
    // an older "preparing" revision.
    if (
      this.cachedSnapshot
      && snapshot.revision < this.cachedSnapshot.revision
    ) return true;
    if (settledConfigOptions(snapshot.agent_config)) {
      this.settledConfigOptions = snapshot.agent_config;
    } else if (
      (!snapshot.agent_config || snapshot.agent_config.status === "loading")
      && this.settledConfigOptions
      && this.settledConfigOptions.agent_id === snapshot.task.agent_id
    ) {
      snapshot = { ...snapshot, agent_config: this.settledConfigOptions };
    }
    if (this.cachedSnapshot === snapshot) return true;
    this.cachedSnapshot = snapshot;
    this.emit();
    return true;
  }

  retain({
    attachmentResources,
    preparationKey,
    snapshot,
  }: {
    attachmentResources?: ComposerAttachmentResourceOwner;
    preparationKey: string;
    snapshot: TaskSnapshot;
  }) {
    if (this.expiredLeaseTaskId === snapshot.task.task_id) this.expiredLeaseTaskId = undefined;
    if (!this.updateSnapshot(snapshot)) return undefined;
    return this.claim({ attachmentResources, preparationKey, taskId: snapshot.task.task_id as TaskId });
  }

  /** Atomically adopts the server replacement without releasing the stale lease first. */
  retainReplacement({
    attachmentResources,
    preparationKey,
    snapshot,
    staleTaskId,
  }: {
    attachmentResources?: ComposerAttachmentResourceOwner;
    preparationKey: string;
    snapshot: TaskSnapshot;
    staleTaskId: TaskId;
  }) {
    if (this.currentTaskId() !== staleTaskId || this.replacementRequired?.taskId !== staleTaskId) {
      return undefined;
    }
    this.generation += 1;
    this.current = undefined;
    this.cachedSnapshot = undefined;
    this.replacementRequired = undefined;
    return this.retain({ attachmentResources, preparationKey, snapshot });
  }

  claim({
    attachmentResources,
    preparationKey,
    taskId,
  }: {
    attachmentResources?: ComposerAttachmentResourceOwner;
    preparationKey: string;
    taskId: TaskId;
  }): NewTaskLease {
    if (this.current?.taskId === taskId) {
      // Prepared Task identity owns the lease. Render-key churn cannot change
      // that Task's immutable context and must not invalidate in-flight work.
      attachmentResources?.claimNewTaskController(taskId);
      return this.current;
    }
    const lease = { generation: ++this.generation, preparationKey, taskId };
    this.current = lease;
    if (this.expiredLeaseTaskId === taskId) this.expiredLeaseTaskId = undefined;
    attachmentResources?.claimNewTaskController(taskId);
    return lease;
  }

  currentLease(taskId?: TaskId | string) {
    if (taskId !== undefined && this.current?.taskId !== taskId) return undefined;
    return this.current;
  }

  currentTaskId() {
    return (this.cachedSnapshot?.task.task_id as TaskId | undefined) ?? this.current?.taskId;
  }

  ownsPreparation(preparationKey: string) {
    return this.current?.preparationKey === preparationKey;
  }

  preparationResetKey() {
    return this.preparationReset;
  }

  taskRequiringReplacement() {
    return this.replacementRequired?.taskId;
  }

  /** Requests the retained New Task context be prepared again after recovery. */
  retryPreparation() {
    this.preparationReset += 1;
    this.emit();
  }

  isCurrent(lease: NewTaskLease) {
    return this.current?.generation === lease.generation;
  }

  /** Protects a send independently from subsequent New Task lease changes. */
  protectSend(lease: NewTaskLease) {
    if (this.isCurrent(lease)) this.current = undefined;
    this.sendProtections.add(lease.taskId);
  }

  settleSend(taskId: TaskId | string) {
    this.sendProtections.delete(taskId as TaskId);
  }

  settleTaskSends(taskId: TaskId | string) {
    this.sendProtections.delete(taskId as TaskId);
  }

  confirmSentTask(taskId: TaskId | string) {
    if (this.current?.taskId === taskId) this.current = undefined;
    if (this.cachedSnapshot?.task.task_id === taskId) {
      this.cachedSnapshot = undefined;
      this.emit();
    }
    this.settleTaskSends(taskId);
  }

  /** Reclaims only the exact lease that started a rejected send. */
  reclaim(
    lease: NewTaskLease,
    attachmentResources?: ComposerAttachmentResourceOwner,
  ) {
    if (this.generation !== lease.generation || this.current) return false;
    this.current = lease;
    attachmentResources?.claimNewTaskController(lease.taskId);
    return true;
  }

  /** Records disposal completed by the first-send cancellation workflow. */
  recordDiscarded(taskId: TaskId) {
    if (this.current?.taskId === taskId) this.current = undefined;
    if (this.cachedSnapshot?.task.task_id === taskId) {
      this.cachedSnapshot = undefined;
      this.emit();
    }
    this.settleTaskSends(taskId);
    if (!this.disposals.has(taskId)) this.disposals.set(taskId, Promise.resolve());
  }

  isDisposable(taskId: TaskId | string) {
    const sendProtected = this.sendProtections.has(taskId as TaskId);
    return !sendProtected && !this.disposals.has(taskId as TaskId);
  }

  /** Drops identities that are not comparable after the state root changes. */
  replaceStateRoot() {
    const hadSnapshot = this.cachedSnapshot !== undefined;
    this.generation += 1;
    this.current = undefined;
    this.expiredLeaseTaskId = undefined;
    this.replacementRequired = undefined;
    this.settledConfigOptions = undefined;
    this.cachedSnapshot = undefined;
    this.disposals.clear();
    this.sendProtections.clear();
    if (hadSnapshot) this.emit();
  }

  /** Forgets only a still-leased Prepared Task after the App Server expires this client. */
  expireClientLease() {
    const lease = this.current;
    if (!lease) return undefined;
    this.generation += 1;
    this.preparationReset += 1;
    this.current = undefined;
    this.expiredLeaseTaskId = lease.taskId;
    this.replacementRequired = undefined;
    if (this.cachedSnapshot?.task.task_id === lease.taskId) this.cachedSnapshot = undefined;
    this.emit();
    return lease.taskId;
  }

  /** Discards an empty Task at most once and only for its current lease. */
  discard({ attachmentResources, dispatch, lease, request, taskId }: NewTaskDisposal) {
    if (!this.isDisposable(taskId)) return Promise.resolve();
    if (lease && !this.isCurrent(lease)) return Promise.resolve();
    const existing = this.disposals.get(taskId);
    if (existing) return existing;
    if (this.current?.taskId === taskId) this.current = undefined;
    if (this.cachedSnapshot?.task.task_id === taskId) {
      this.cachedSnapshot = undefined;
      this.emit();
    }
    attachmentResources?.releaseTask(taskId);
    // Local ownership ends synchronously; backend acknowledgement may settle after routing.
    dispatch({ type: "taskInput:clear", taskId });
    dispatch({ type: "task:list:remove", taskId });
    const disposal = (async () => {
      try {
        await request?.(TASK_RELEASE, { taskId });
      } catch {
        // Explicit discard already ended local ownership; remote cleanup is best effort.
      }
    })();
    this.disposals.set(taskId, disposal);
    return disposal;
  }

  private emit() {
    for (const listener of this.listeners) listener();
  }
}

function settledConfigOptions(catalog: ConfigOptionsCatalog | undefined) {
  return catalog?.status === "ready" || catalog?.status === "empty";
}

/** Returns only a genuinely disposable New Task snapshot, never a protected send. */
export function disposableNewTaskControllerId(
  state: AppState,
  controller: NewTaskController,
): TaskId | undefined {
  if (state.newTask.submitting || state.snapshot?.lifecycle !== "prepared") return undefined;
  const taskId = state.snapshot.task.task_id as TaskId;
  if (controller.currentTaskId() !== taskId || !controller.isDisposable(taskId)) return undefined;
  return taskId;
}
