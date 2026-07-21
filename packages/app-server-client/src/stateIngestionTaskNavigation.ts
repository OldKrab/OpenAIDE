import type {
  SubscriptionScope,
  TaskNavigationEntry,
  TaskNavigationSnapshot,
  TaskSummary,
} from "./generated/protocol.js";

export function filterTaskNavigationForScope(
  navigation: TaskNavigationSnapshot,
  scope: SubscriptionScope,
): TaskNavigationSnapshot {
  if (scope.kind !== "taskNavigation" || scope.projectId === null || scope.projectId === undefined) return navigation;

  const entries = navigation.entries.filter((entry) => (
    entry.kind === "task"
      ? entry.task.projectId === scope.projectId
      : entry.session.projectId === scope.projectId
  ));
  const activeTaskId =
    navigation.activeTaskId !== null && navigation.activeTaskId !== undefined
      ? entries.some((entry) => entry.kind === "task" && entry.task.taskId === navigation.activeTaskId)
        ? navigation.activeTaskId
        : null
      : navigation.activeTaskId;

  return { ...navigation, entries, activeTaskId };
}

/** Keeps the combined Navigation projection coherent while focused Task events arrive. */
export function upsertTaskNavigationEntry(
  entries: TaskNavigationEntry[],
  task: TaskSummary,
): TaskNavigationEntry[] {
  const next = entries.filter((entry) => entry.kind !== "task" || entry.task.taskId !== task.taskId);
  next.push({ kind: "task", task });
  return next.sort((left, right) => entryActivity(right).localeCompare(entryActivity(left)));
}

export function removeTaskNavigationEntry(
  entries: TaskNavigationEntry[],
  taskId: TaskSummary["taskId"],
): TaskNavigationEntry[] {
  return entries.filter((entry) => entry.kind !== "task" || entry.task.taskId !== taskId);
}

function entryActivity(entry: TaskNavigationEntry): string {
  return entry.kind === "task" ? entry.task.lastActivity : entry.session.lastActivity ?? "";
}
