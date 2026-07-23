import type {
  ProjectId,
  SubscriptionScope,
  TaskNavigationEntry,
  TaskNavigationSnapshot,
  TaskSummary,
} from "./generated/protocol.js";

/** Restricts one authoritative Navigation baseline without regrouping or sorting it. */
export function filterTaskNavigationForScope(
  navigation: TaskNavigationSnapshot,
  scope: SubscriptionScope,
): TaskNavigationSnapshot {
  if (scope.kind !== "taskNavigation") return navigation;
  const selected = scope.projectIds;
  if (selected === null || selected === undefined) return navigation;
  return {
    ...navigation,
    groups: navigation.groups.filter((group) => selected.includes(group.projectId)),
  };
}

/**
 * Updates row-visible state only when the Task already belongs to this replica.
 * Membership changes require an authoritative projectEntriesReplaced event.
 */
export function updateExistingNavigationTask(
  navigation: TaskNavigationSnapshot,
  projectId: ProjectId,
  task: TaskSummary,
): TaskNavigationSnapshot | undefined {
  const groupIndex = navigation.groups.findIndex((group) => group.projectId === projectId);
  if (groupIndex < 0) return undefined;
  const group = navigation.groups[groupIndex]!;
  const entryIndex = group.entries.findIndex(
    (entry) => entry.kind === "task" && entry.task.taskId === task.taskId,
  );
  if (entryIndex < 0) return undefined;

  const entries = group.entries.slice();
  entries[entryIndex] = { kind: "task", task };
  const groups = navigation.groups.slice();
  groups[groupIndex] = { ...group, entries };
  return { ...navigation, groups };
}

/** Replaces one Project's complete loaded row window while preserving group order and label. */
export function replaceNavigationProjectEntries(
  navigation: TaskNavigationSnapshot,
  projectId: ProjectId,
  taskCount: number,
  entries: TaskNavigationEntry[],
  hasMore: boolean,
): TaskNavigationSnapshot | undefined {
  const groupIndex = navigation.groups.findIndex((group) => group.projectId === projectId);
  if (groupIndex < 0) return undefined;
  const groups = navigation.groups.slice();
  groups[groupIndex] = {
    ...groups[groupIndex]!,
    taskCount,
    entries,
    hasMore,
  };
  return { ...navigation, groups };
}
