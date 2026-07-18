import type { AgentListedSession } from "@openaide/app-shell-contracts";
import { ChevronDown, ChevronRight, GitBranch, MoreHorizontal, Plus } from "lucide-react";
import { useEffect, useRef, useState } from "react";
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
  canManageWorktrees: boolean;
  onArchiveTask: (taskId: string) => void;
  onLoadMore: (visibleIncrement: number) => void;
  onManageWorktrees?: () => void;
  onNewTask: () => void;
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
  canManageWorktrees,
  onArchiveTask,
  onLoadMore,
  onManageWorktrees,
  onNewTask,
  onOpenNativeSession,
  onOpenTask,
  onRestoreTask,
  onToggleCollapse,
  showArchived,
}: SidebarProjectTaskGroupProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const actionsRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!menuOpen) return undefined;
    const dismiss = (event: PointerEvent) => {
      if (!actionsRef.current?.contains(event.target as Node)) setMenuOpen(false);
    };
    document.addEventListener("pointerdown", dismiss);
    return () => document.removeEventListener("pointerdown", dismiss);
  }, [menuOpen]);
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
        <div className="project-task-group-actions" ref={actionsRef}>
          <button aria-expanded={menuOpen} aria-label={`${group.label} actions`} onClick={() => setMenuOpen((open) => !open)} type="button"><MoreHorizontal size={14} /></button>
          {menuOpen ? <div className="project-task-group-menu" role="menu">
            <button onClick={() => { setMenuOpen(false); onNewTask(); }} role="menuitem" type="button"><Plus size={13} />New task</button>
            {canManageWorktrees && onManageWorktrees ? <button onClick={() => { setMenuOpen(false); onManageWorktrees(); }} role="menuitem" type="button"><GitBranch size={13} />Manage worktrees</button> : null}
          </div> : null}
        </div>
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
