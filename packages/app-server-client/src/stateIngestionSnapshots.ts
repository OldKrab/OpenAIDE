import type {
  AppServerEventPayload,
  ClientSnapshot,
  SubscriptionScope,
  SubscriptionSnapshot,
} from "./generated/protocol.js";
import { filterTaskNavigationForScope, upsertTaskSummary } from "./stateIngestionTaskNavigation.js";
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
      return payload.kind === "toolDetailUpdated"
        && payload.taskId === snapshot.taskId
        && payload.artifactId === snapshot.artifactId
        ? changed({ ...snapshot, details: payload.details })
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
      return unchanged(snapshot);
  }
}

function updateTaskNavigationSnapshot(
  scope: SubscriptionScope,
  snapshot: Extract<SubscriptionSnapshot, { kind: "taskNavigation" }>,
  payload: AppServerEventPayload,
): SnapshotUpdate {
  if (payload.kind === "taskNavigationChanged") {
    const change = payload.change;
    if (change.kind === "remove") {
      return changed({
        ...snapshot,
        navigation: {
          ...snapshot.navigation,
          tasks: snapshot.navigation.tasks.filter((task) => task.taskId !== change.taskId),
        },
      });
    }
    const matchesProject = scope.kind !== "taskNavigation"
      || scope.projectId === null
      || scope.projectId === undefined
      || change.task.projectId === scope.projectId;
    if (!matchesProject) return unchanged(snapshot);
    return changed({
      ...snapshot,
      navigation: {
        ...snapshot.navigation,
        tasks: upsertTaskSummary(snapshot.navigation.tasks, change.task),
      },
    });
  }

  return unchanged(snapshot);
}
