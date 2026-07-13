import type { AgentListedSession } from "@openaide/app-shell-contracts";
import { ChevronDown, ChevronRight } from "lucide-react";
import { SidebarNativeSessionRow } from "./SidebarNativeSessionRow";
import { SidebarTaskRow } from "./SidebarTaskRow";
import {
  projectGroupRows,
  recentVisibleRows,
  type SidebarProjectGroup,
} from "./sidebarProjectModel";

type SidebarProjectTaskGroupProps = {
  activeTaskId?: string;
  collapsed: boolean;
  group: SidebarProjectGroup;
  maxTasks: number;
  pageSize: number;
  nativeSessionAgentId: string;
  nativeSessionAgentName: string;
  nativeSessions: AgentListedSession[];
  nativeSessionsAdoptingSessionId?: string;
  nativeSessionsHaveMore: boolean;
  onArchiveTask: (taskId: string) => void;
  onLoadMore: (visibleIncrement: number) => void;
  onOpenNativeSession: (session: AgentListedSession) => void;
  onOpenTask: (taskId: string) => void;
  onRestoreTask: (taskId: string) => void;
  onToggleCollapse: () => void;
  showArchived: boolean;
};

export function SidebarProjectTaskGroup({
  activeTaskId,
  collapsed,
  group,
  maxTasks,
  pageSize,
  nativeSessionAgentId,
  nativeSessionAgentName,
  nativeSessions,
  nativeSessionsAdoptingSessionId,
  nativeSessionsHaveMore,
  onArchiveTask,
  onLoadMore,
  onOpenNativeSession,
  onOpenTask,
  onRestoreTask,
  onToggleCollapse,
  showArchived,
}: SidebarProjectTaskGroupProps) {
  const activeTask = group.tasks.find((task) => task.task_id === activeTaskId);
  const taskRows = projectGroupRows(group.tasks, []);
  const allRows = projectGroupRows(group.tasks, nativeSessions);
  const activeRow = activeTask
    ? allRows.find((row) => row.kind === "task" && row.task.task_id === activeTask.task_id)
    : undefined;
  const visibleRows = recentVisibleRows(allRows, maxTasks, activeRow);
  const renderedRows = collapsed ? (activeRow ? [activeRow] : []) : visibleRows;
  const hiddenCount = Math.max(0, allRows.length - visibleRows.length);
  const countSummary = projectGroupCountSummary(taskRows.length, nativeSessions.length);

  return (
    <section className="project-task-group" aria-label={group.label}>
      <div className="project-task-group-header">
        <button
          aria-expanded={!collapsed}
          className="project-task-group-toggle"
          onClick={onToggleCollapse}
          type="button"
        >
          {collapsed ? <ChevronRight size={13} /> : <ChevronDown size={13} />}
          <span>
            <strong>{group.label}</strong>
            {countSummary ? <small className="project-task-group-counts">{countSummary}</small> : null}
          </span>
        </button>
      </div>
      {renderedRows.map((row) =>
        row.kind === "task" ? (
          <SidebarTaskRow
            key={`task:${row.task.task_id}`}
            activeTaskId={activeTaskId}
            onArchiveTask={onArchiveTask}
            onOpenTask={onOpenTask}
            onRestoreTask={onRestoreTask}
            showArchived={showArchived}
            task={row.task}
          />
        ) : (
          <SidebarNativeSessionRow
            key={`session:${row.session.session_id}`}
            nativeSessionAgentId={nativeSessionAgentId}
            nativeSessionAgentName={nativeSessionAgentName}
            nativeSessionsAdoptingSessionId={nativeSessionsAdoptingSessionId}
            onOpenNativeSession={onOpenNativeSession}
            session={row.session}
          />
        ),
      )}
      {!collapsed && (hiddenCount > 0 || nativeSessionsHaveMore) ? (
        <button
          className="project-task-more"
          onClick={() => onLoadMore(hiddenCount > 0 ? Math.min(pageSize, hiddenCount) : pageSize)}
          type="button"
        >
          {hiddenCount > 0 ? `Load ${Math.min(pageSize, hiddenCount)} more tasks` : "Load more tasks"}
        </button>
      ) : null}
    </section>
  );
}

function projectGroupCountSummary(taskCount: number, sessionCount: number) {
  const totalCount = taskCount + sessionCount;
  return totalCount ? `${totalCount} ${totalCount === 1 ? "task" : "tasks"}` : "";
}
