import type {
  AppServerEventPayload,
  SubscriptionScope,
} from "./generated/protocol.js";
import type { ReplicaSnapshot } from "./backendConnection.js";
import { applyNavigationFrame } from "./stateIngestionTaskNavigation.js";
import { updateSubagentHistorySnapshot, updateToolDetailSnapshot } from "./stateIngestionEntities.js";
import { updateTaskSnapshot } from "./stateIngestionTask.js";
import type { SnapshotUpdate } from "./stateIngestionTypes.js";
import { changed, unchanged } from "./stateIngestionTypes.js";

export function updateSubscriptionSnapshot(
  scope: SubscriptionScope,
  snapshot: ReplicaSnapshot,
  payload: AppServerEventPayload,
): SnapshotUpdate {
  switch (snapshot.kind) {
    case "taskNavigation":
      return updateTaskNavigationSnapshot(snapshot, payload);
    case "task":
      return updateTaskSnapshot(snapshot, payload);
    case "subagentCatalog":
      return payload.kind === "subagentCatalogUpdated"
        && payload.catalog.taskId === snapshot.catalog.taskId
        && payload.catalog.revision >= snapshot.catalog.revision
        ? changed({ kind: "subagentCatalog", catalog: payload.catalog })
        : unchanged(snapshot);
    case "subagentHistory":
      return updateSubagentHistorySnapshot(snapshot, payload);
    case "toolDetail":
      return updateToolDetailSnapshot(snapshot, payload);
  }
}

function updateTaskNavigationSnapshot(
  snapshot: Extract<ReplicaSnapshot, { kind: "taskNavigation" }>,
  payload: AppServerEventPayload,
): SnapshotUpdate {
  if (payload.kind !== "navigationFrame") return unchanged(snapshot);
  const applied = applyNavigationFrame(snapshot.navigation, payload.frame);
  if (applied.kind === "stale") return unchanged(snapshot);
  if (applied.kind === "resyncRequired") return { kind: "resyncRequired", reason: applied.reason, detail: applied.detail };
  return changed({ kind: "taskNavigation", navigation: applied.navigation });
}
