import type { SnapshotIntent } from "./appReducer";
import type { WebviewBootstrap } from "./surfaceTypes";

/** Opaque proof that an asynchronous result still belongs to its latest caller. */
export type AsyncOperationLease = Readonly<{
  context?: string;
  id: number;
  lifetime: "navigation" | "replica";
  replica: number;
  owner: string;
  navigation?: number;
}>;

/**
 * Owns Frontend asynchronous-result ordering.
 *
 * Workflows claim one lease and ask whether it still owns settlement. Snapshot
 * ordering stays private because its per-Task/open rules are more specific than
 * ordinary latest-request-wins behavior.
 */
export class AsyncOperationOwner {
  private nextId = 0;
  private navigation = 0;
  private replica = 0;
  private currentNavigationTarget: string | undefined;
  private pendingNavigationTarget: string | undefined;
  private archived = false;
  private latestOperationByOwner = new Map<string, number>();
  private latestOperationContextByOwner = new Map<string, string | undefined>();
  private latestOperationLifetimeByOwner = new Map<string, AsyncOperationLease["lifetime"]>();
  private latestSnapshotByTask = new Map<string, number>();
  private latestOpenSnapshotId: number | undefined;
  private snapshotNavigation = new Map<number, number>();

  claim(
    owner: string,
    context?: string,
    lifetime: AsyncOperationLease["lifetime"] = "navigation",
  ): AsyncOperationLease {
    const id = this.nextId + 1;
    this.nextId = id;
    this.latestOperationByOwner.set(owner, id);
    this.latestOperationContextByOwner.set(owner, context);
    this.latestOperationLifetimeByOwner.set(owner, lifetime);
    return this.lease(owner, id, context, lifetime);
  }

  /** Returns the stable lease for a logical context, replacing it only when that context changes. */
  scope(
    owner: string,
    context: string,
    lifetime: AsyncOperationLease["lifetime"] = "navigation",
  ): AsyncOperationLease {
    const currentId = this.latestOperationByOwner.get(owner);
    if (
      currentId !== undefined
      && this.latestOperationContextByOwner.get(owner) === context
      && this.latestOperationLifetimeByOwner.get(owner) === lifetime
    ) {
      return this.lease(owner, currentId, context, lifetime);
    }
    return this.claim(owner, context, lifetime);
  }

  owns(lease: AsyncOperationLease): boolean {
    return lease.replica === this.replica
      && (lease.lifetime === "replica" || lease.navigation === this.navigation)
      && this.latestOperationByOwner.get(lease.owner) === lease.id;
  }

  /** Invalidates current work immediately and records the route expected from the shell. */
  beginNavigation(target?: string, archived?: boolean): void {
    this.invalidateNavigation();
    this.pendingNavigationTarget = target;
    if (archived !== undefined) this.archived = archived;
  }

  /** Records a target learned only after an asynchronous navigation operation completes. */
  expectNavigation(target: string): void {
    this.pendingNavigationTarget = target;
  }

  /** Adopts a shell route, consuming a matching Frontend intent without invalidating twice. */
  observeNavigation(target: string, archived?: boolean): void {
    if (archived !== undefined) this.archived = archived;
    if (this.pendingNavigationTarget === target) {
      this.pendingNavigationTarget = undefined;
      this.currentNavigationTarget = target;
      return;
    }
    if (this.currentNavigationTarget === target) return;
    this.invalidateNavigation();
    this.currentNavigationTarget = target;
  }

  /** Invalidates work whose result belongs to a replaced App Server replica. */
  replaceReplica(): void {
    this.replica += 1;
    this.invalidateNavigation();
    this.latestOperationByOwner.clear();
    this.latestOperationContextByOwner.clear();
    this.latestOperationLifetimeByOwner.clear();
    this.pendingNavigationTarget = undefined;
  }

  currentArchived(): boolean {
    return this.archived;
  }

  createSnapshotRequest(taskId?: string, intent: SnapshotIntent = "refresh"): number {
    const requestId = this.nextId + 1;
    this.nextId = requestId;
    if (intent === "open") this.latestOpenSnapshotId = requestId;
    if (taskId) this.latestSnapshotByTask.set(taskId, requestId);
    this.snapshotNavigation.set(requestId, this.navigation);
    return requestId;
  }

  acceptSnapshot(taskId: string, requestId: number | undefined, intent: SnapshotIntent): boolean {
    if (requestId !== undefined) {
      const requestNavigation = this.snapshotNavigation.get(requestId);
      this.snapshotNavigation.delete(requestId);
      if (requestNavigation === undefined || requestNavigation !== this.navigation) return false;
      if (intent === "open" && requestId !== this.latestOpenSnapshotId) return false;
      const latestTaskRequest = this.latestSnapshotByTask.get(taskId);
      if (latestTaskRequest !== undefined && requestId < latestTaskRequest) return false;
    }
    return true;
  }

  private invalidateNavigation(): void {
    this.navigation += 1;
    this.latestSnapshotByTask.clear();
    this.latestOpenSnapshotId = undefined;
    this.snapshotNavigation.clear();
  }

  private lease(
    owner: string,
    id: number,
    context: string | undefined,
    lifetime: AsyncOperationLease["lifetime"],
  ): AsyncOperationLease {
    return {
      context,
      id,
      lifetime,
      owner,
      replica: this.replica,
      ...(lifetime === "navigation" ? { navigation: this.navigation } : {}),
    };
  }
}

/** Stable product-surface identity; it contains no browser or VS Code route shape. */
export function navigationTargetForBootstrap(bootstrap: WebviewBootstrap): string {
  switch (bootstrap.surface) {
    case "task":
      return bootstrap.taskId
        ? taskNavigationTarget(bootstrap.taskId)
        : newTaskNavigationTarget(bootstrap.projectId);
    case "nativeSession":
      return nativeSessionNavigationTarget(bootstrap.agentId, bootstrap.nativeSessionId);
    case "settings":
      return `settings:${bootstrap.settingsTab ?? "default"}`;
    case "navigation":
      return taskListNavigationTarget(bootstrap.archived === true);
    case "invalid":
      return "invalid";
  }
}

export function nativeSessionNavigationTarget(agentId?: string, nativeSessionId?: string): string {
  return `native-session:${agentId ?? "missing"}:${nativeSessionId ?? "missing"}`;
}

export function taskNavigationTarget(taskId: string): string {
  return `task:${taskId}`;
}

export function newTaskNavigationTarget(projectId?: string): string {
  return `new-task:${projectId ?? "default"}`;
}

export function taskListNavigationTarget(archived: boolean): string {
  return `navigation:${archived ? "archived" : "active"}`;
}

export function settingsNavigationTarget(tab = "default"): string {
  return `settings:${tab}`;
}
