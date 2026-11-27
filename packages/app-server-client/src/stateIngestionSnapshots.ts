import type {
  AppServerEventPayload,
  ClientSnapshot,
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
  if (payload.kind === "snapshotReplaced") {
    return updateFromClientSnapshot(scope, snapshot, payload.snapshot);
  }

  switch (snapshot.kind) {
    case "projects":
      return payload.kind === "projectCollectionUpdated"
        ? changed({ kind: "projects", projects: payload.projects })
        : unchanged(snapshot);
    case "agents":
      return payload.kind === "agentCollectionUpdated"
        ? changed({ kind: "agents", agents: payload.agents })
        : unchanged(snapshot);
    case "devices":
      return payload.kind === "deviceCollectionUpdated"
        ? changed({ kind: "devices", devices: payload.devices })
        : unchanged(snapshot);
    case "terminals":
      return payload.kind === "terminalCollectionUpdated"
        ? changed({ kind: "terminals", terminals: payload.terminals })
        : unchanged(snapshot);
    case "settings":
      return unchanged(snapshot);
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
    case "worktreeRepository":
      return payload.kind === "worktreeRepositoryUpdated"
        && payload.repositoryId === snapshot.repository.repositoryId
        ? changed({ kind: "worktreeRepository", repository: payload.repository })
        : unchanged(snapshot);
  }
}

function updateFromClientSnapshot(
  scope: SubscriptionScope,
  snapshot: ReplicaSnapshot,
  clientSnapshot: ClientSnapshot,
): SnapshotUpdate {
  switch (snapshot.kind) {
    case "projects":
      return clientSnapshot.projects ? changed({ kind: "projects", projects: clientSnapshot.projects }) : unchanged(snapshot);
    case "agents":
      return clientSnapshot.agents ? changed({ kind: "agents", agents: clientSnapshot.agents }) : unchanged(snapshot);
    case "settings":
      return clientSnapshot.settings ? changed({ kind: "settings", settings: clientSnapshot.settings }) : unchanged(snapshot);
    case "taskNavigation":
      // A view is only ever built from its own frames.
      return unchanged(snapshot);
    case "task":
      // A Client snapshot's active Task has no Chat version, so it cannot replace the replica;
      // the Task's own stream carries every change to it.
      return unchanged(snapshot);
    case "subagentCatalog":
    case "subagentHistory":
      return unchanged(snapshot);
    case "devices":
    case "terminals":
    case "toolDetail":
    case "worktreeRepository":
      return unchanged(snapshot);
  }
}

function updateTaskNavigationSnapshot(
  snapshot: Extract<ReplicaSnapshot, { kind: "taskNavigation" }>,
  payload: AppServerEventPayload,
): SnapshotUpdate {
  if (payload.kind !== "navigationFrame") return unchanged(snapshot);
  const applied = applyNavigationFrame(snapshot.navigation, payload.frame);
  if (applied.kind === "resyncRequired") return { kind: "resyncRequired", reason: applied.reason };
  return changed({ kind: "taskNavigation", navigation: applied.navigation });
}
