import type { SnapshotIntent } from "./appReducer";

export type SnapshotAcceptResult =
  | { accepted: true }
  | {
      accepted: false;
      reason: "unknown_request" | "stale_generation" | "stale_open_request" | "older_task_request";
      latestSnapshotRequestId?: number;
    };

export class SnapshotRequestTracker {
  private nextRequestId = 0;
  private latestByTask: Record<string, number> = {};
  private latestOpenRequestId: number | undefined;
  private generationByRequest: Record<number, number> = {};
  private generation = 0;
  private archived = false;

  create(taskId?: string, intent: SnapshotIntent = "refresh") {
    const requestId = this.nextRequestId + 1;
    this.nextRequestId = requestId;
    if (intent === "open") {
      this.latestOpenRequestId = requestId;
    }
    if (taskId) {
      this.latestByTask[taskId] = requestId;
    }
    this.generationByRequest[requestId] = this.generation;
    return requestId;
  }

  beginNavigationChange(archived?: boolean) {
    this.generation += 1;
    this.latestByTask = {};
    this.latestOpenRequestId = undefined;
    this.generationByRequest = {};
    if (archived !== undefined) {
      this.archived = archived;
    }
  }

  currentArchived() {
    return this.archived;
  }

  accept(taskId: string, requestId: number | undefined, intent: SnapshotIntent): SnapshotAcceptResult {
    if (requestId !== undefined) {
      if (!(requestId in this.generationByRequest)) {
        return { accepted: false, reason: "unknown_request" };
      }
      const requestGeneration = this.generationByRequest[requestId];
      delete this.generationByRequest[requestId];
      if (requestGeneration !== undefined && requestGeneration !== this.generation) {
        return { accepted: false, reason: "stale_generation" };
      }
    }
    if (intent === "open" && requestId !== undefined && requestId !== this.latestOpenRequestId) {
      return {
        accepted: false,
        reason: "stale_open_request",
        latestSnapshotRequestId: this.latestOpenRequestId,
      };
    }
    const latestRequestId = this.latestByTask[taskId];
    if (requestId !== undefined && latestRequestId !== undefined && requestId < latestRequestId) {
      return {
        accepted: false,
        reason: "older_task_request",
        latestSnapshotRequestId: latestRequestId,
      };
    }
    return { accepted: true };
  }
}
