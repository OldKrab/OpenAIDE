import type {
  AppServerEventPayload,
  ClientSnapshot,
  SubscriptionScope,
  SubscriptionSnapshot,
} from "./generated/protocol.js";
import {
  filterTaskNavigationForScope,
  replaceNavigationProjectEntries,
  updateExistingNavigationTask,
} from "./stateIngestionTaskNavigation.js";
import { updateTaskSnapshot } from "./stateIngestionTask.js";
import type { SnapshotUpdate } from "./stateIngestionTypes.js";
import { changed, unchanged } from "./stateIngestionTypes.js";

export function updateSubscriptionSnapshot(
  scope: SubscriptionScope,
  snapshot: SubscriptionSnapshot,
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
    case "settings":
      return unchanged(snapshot);
    case "taskNavigation":
      return updateTaskNavigationSnapshot(scope, snapshot, payload);
    case "task":
      return updateTaskSnapshot(snapshot, payload);
    case "toolDetail":
      if ((payload.kind !== "toolDetailUpdated" && payload.kind !== "toolDetailChanged")
        || payload.taskId !== snapshot.taskId
        || payload.artifactId !== snapshot.artifactId) {
        return unchanged(snapshot);
      }
      if (payload.kind === "toolDetailUpdated") {
        if (payload.details.revision < snapshot.details.revision) return unchanged(snapshot);
        return changed({ ...snapshot, details: payload.details });
      }
      if (payload.kind === "toolDetailChanged") {
        // A baseline can race with dispatch of the durable delta it already contains.
        if (payload.revision <= snapshot.details.revision) return unchanged(snapshot);
        if (payload.revision !== snapshot.details.revision + 1) {
          return { kind: "resyncRequired", reason: "toolDetailRevisionGap" };
        }
        const terminalOutputs = (snapshot.details.terminalOutputs ?? []).map((terminal) => ({ ...terminal }));
        let details = snapshot.details;
        for (const delta of payload.deltas) {
          if (delta.kind === "replaceDetails") {
            details = {
              ...delta.details,
              revision: payload.revision,
              terminalOutputs,
            };
            continue;
          }
          const existing = terminalOutputs.find((terminal) => terminal.terminalId === delta.terminalId);
          if (existing) existing.output += delta.data;
          else terminalOutputs.push({ terminalId: delta.terminalId, output: delta.data });
        }
        return changed({
          ...snapshot,
          details: { ...details, revision: payload.revision, terminalOutputs },
        });
      }
      return unchanged(snapshot);
    case "worktreeRepository":
      return payload.kind === "worktreeRepositoryUpdated"
        && payload.repositoryId === snapshot.repository.repositoryId
        ? changed({ kind: "worktreeRepository", repository: payload.repository })
        : unchanged(snapshot);
  }
}

function updateFromClientSnapshot(
  scope: SubscriptionScope,
  snapshot: SubscriptionSnapshot,
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
      return clientSnapshot.tasks
        ? changed({ kind: "taskNavigation", navigation: filterTaskNavigationForScope(clientSnapshot.tasks, scope) })
        : unchanged(snapshot);
    case "task":
      return clientSnapshot.activeTask && clientSnapshot.activeTask.task.taskId === snapshot.task.task.taskId
        ? changed({ kind: "task", task: clientSnapshot.activeTask })
        : unchanged(snapshot);
    case "toolDetail":
    case "worktreeRepository":
      return unchanged(snapshot);
  }
}

function updateTaskNavigationSnapshot(
  scope: SubscriptionScope,
  snapshot: Extract<SubscriptionSnapshot, { kind: "taskNavigation" }>,
  payload: AppServerEventPayload,
): SnapshotUpdate {
  if (payload.kind === "navigationReplaced") {
    return changed({
      ...snapshot,
      navigation: filterTaskNavigationForScope(payload.navigation, scope),
    });
  }
  if (payload.kind === "refreshStateChanged") {
    return changed({
      ...snapshot,
      navigation: {
        ...snapshot.navigation,
        refresh: payload.refresh,
      },
    });
  }
  if (payload.kind === "taskUpdated") {
    const navigation = updateExistingNavigationTask(
      snapshot.navigation,
      payload.projectId,
      payload.task,
    );
    return navigation ? changed({ ...snapshot, navigation }) : unchanged(snapshot);
  }
  if (payload.kind === "projectEntriesReplaced") {
    if (payload.section !== snapshot.navigation.section) return unchanged(snapshot);
    const navigation = replaceNavigationProjectEntries(
      snapshot.navigation,
      payload.projectId,
      payload.taskCount,
      payload.entries,
      payload.hasMore,
    );
    return navigation ? changed({ ...snapshot, navigation }) : unchanged(snapshot);
  }

  return unchanged(snapshot);
}
