import type { AgentListedSession, TaskSummary } from "@openaide/app-shell-contracts";
import type { ProjectOption } from "../state/composerOptions";

export type SidebarProjectGroup = {
  key: string;
  label: string;
  tasks: TaskSummary[];
};

export type SidebarProjectRow =
  | { kind: "task"; task: TaskSummary; timestamp: string | undefined }
  | { kind: "session"; session: AgentListedSession; timestamp: string | undefined };

export function groupedTasks(
  tasks: TaskSummary[],
  projects: ProjectOption[],
  options: { includeProjectId?: string; includedProjectSessions?: AgentListedSession[] } = {},
): SidebarProjectGroup[] {
  const projectLabels = new Map(projects.map((project) => [project.projectId, project.label]));
  const groups = new Map<string, SidebarProjectGroup>();
  for (const project of projects) {
    groups.set(project.projectId, { key: project.projectId, label: project.label, tasks: [] });
  }
  if (options.includeProjectId && !groups.has(options.includeProjectId)) {
    groups.set(options.includeProjectId, {
      key: options.includeProjectId,
      label: projectLabels.get(options.includeProjectId) ?? "Current workspace",
      tasks: [],
    });
  }
  for (const task of tasks) {
    const key = task.project_id ?? (task.workspace_root || "unknown");
    const label = task.project_label
      ?? (task.project_id ? projectLabels.get(task.project_id) : undefined)
      ?? (task.workspace_root || "Other workspace");
    const group = groups.get(key) ?? { key, label, tasks: [] };
    group.tasks.push(task);
    groups.set(key, group);
  }
  return [...groups.values()].sort((left, right) =>
    compareInProgressDesc(groupHasInProgress(left), groupHasInProgress(right))
    || compareActivityDesc(newestGroupActivity(left, options), newestGroupActivity(right, options))
    || left.label.localeCompare(right.label)
    || left.key.localeCompare(right.key),
  );
}

export function taskMatchesSearch(task: TaskSummary, query: string) {
  return [
    task.title,
    task.agent_name,
    task.project_label,
    task.workspace_root,
  ].some((value) => value?.toLowerCase().includes(query));
}

export function projectGroupRows(tasks: TaskSummary[], sessions: AgentListedSession[]): SidebarProjectRow[] {
  return [
    ...tasks.map((task) => ({ kind: "task" as const, task, timestamp: taskNavigationTimestamp(task) })),
    ...sessions.map((session) => ({ kind: "session" as const, session, timestamp: sessionActivityTimestamp(session) })),
  ].sort(compareProjectGroupRows);
}

export function recentVisibleRows(rows: SidebarProjectRow[], maxRows: number, activeRow?: SidebarProjectRow) {
  const visible = rows.slice(0, maxRows);
  if (activeRow && !visible.some((row) => sameProjectGroupRow(row, activeRow))) {
    return [activeRow, ...visible.slice(0, Math.max(0, maxRows - 1))];
  }
  return visible;
}

export function recentVisibleGroups(
  groups: SidebarProjectGroup[],
  maxGroups: number,
  activeProjectKey?: string,
) {
  const visible = groups.slice(0, maxGroups);
  const activeGroup = activeProjectKey ? groups.find((group) => group.key === activeProjectKey) : undefined;
  if (activeGroup && !visible.some((group) => group.key === activeGroup.key)) {
    return [activeGroup, ...visible.slice(0, Math.max(0, maxGroups - 1))];
  }
  return visible;
}

function newestActivity(tasks: TaskSummary[]) {
  return tasks.reduce((newest, task) => newerActivity(newest, taskNavigationTimestamp(task)), "");
}

function newestGroupActivity(
  group: SidebarProjectGroup,
  options: { includeProjectId?: string; includedProjectSessions?: AgentListedSession[] },
) {
  const sessionActivity = options.includeProjectId === group.key
    ? newestSessionActivity(options.includedProjectSessions ?? [])
    : "";
  return newerActivity(newestActivity(group.tasks), sessionActivity);
}

function newestSessionActivity(sessions: AgentListedSession[]) {
  return sessions.reduce((newest, session) => newerActivity(newest, sessionActivityTimestamp(session)), "");
}

function compareProjectGroupRows(left: SidebarProjectRow, right: SidebarProjectRow) {
  return compareInProgressDesc(rowInProgress(left), rowInProgress(right))
    || compareActivityDesc(left.timestamp, right.timestamp);
}

function groupHasInProgress(group: SidebarProjectGroup) {
  return group.tasks.some(taskInProgress);
}

function rowInProgress(row: SidebarProjectRow) {
  return row.kind === "task" && taskInProgress(row.task);
}

function taskInProgress(task: TaskSummary) {
  return task.status === "active";
}

function compareInProgressDesc(left: boolean, right: boolean) {
  return Number(right) - Number(left);
}

function taskNavigationTimestamp(task: TaskSummary) {
  return firstTimestamp(task.last_activity, task.updated_at, task.created_at);
}

function sessionActivityTimestamp(session: AgentListedSession) {
  return firstTimestamp(session.last_activity, session.updated_at);
}

function compareActivityDesc(left: string | undefined, right: string | undefined) {
  const leftValue = left ?? "";
  const rightValue = right ?? "";
  return activityMillis(rightValue) - activityMillis(leftValue) || rightValue.localeCompare(leftValue);
}

function newerActivity(left: string | undefined, right: string | undefined): string {
  return compareActivityDesc(left, right) <= 0 ? left ?? "" : right ?? "";
}

function firstTimestamp(...values: Array<string | undefined>) {
  return values.find((value) => value?.trim());
}

function activityMillis(value: string | undefined) {
  const trimmed = value?.trim() ?? "";
  if (!trimmed) return 0;
  const parsed = /^\d+$/.test(trimmed) ? Number(trimmed) : Date.parse(trimmed);
  return Number.isFinite(parsed) ? parsed : 0;
}

function sameProjectGroupRow(left: SidebarProjectRow, right: SidebarProjectRow) {
  if (left.kind !== right.kind) return false;
  if (left.kind === "task" && right.kind === "task") {
    return left.task.task_id === right.task.task_id;
  }
  if (left.kind === "session" && right.kind === "session") {
    return left.session.session_id === right.session.session_id;
  }
  return false;
}
