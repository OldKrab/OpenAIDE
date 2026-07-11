import type { SubscriptionScope, TaskNavigationSnapshot, TaskSummary } from "./generated/protocol.js";

export function filterTaskNavigationForScope(
  navigation: TaskNavigationSnapshot,
  scope: SubscriptionScope,
): TaskNavigationSnapshot {
  if (scope.kind !== "taskNavigation" || scope.projectId === null || scope.projectId === undefined) return navigation;

  const tasks = navigation.tasks.filter((task) => task.projectId === scope.projectId);
  const activeTaskId =
    navigation.activeTaskId !== null && navigation.activeTaskId !== undefined
      ? tasks.some((task) => task.taskId === navigation.activeTaskId)
        ? navigation.activeTaskId
        : null
      : navigation.activeTaskId;

  return { ...navigation, tasks, activeTaskId };
}

export function upsertTaskSummary(tasks: TaskSummary[], task: TaskSummary): TaskSummary[] {
  const existing = tasks.findIndex((candidate) => candidate.taskId === task.taskId);
  if (existing === -1) return [task, ...tasks];

  return tasks.map((candidate, index) => (index === existing ? task : candidate));
}
