import type { AgentListedSession } from "@openaide/app-shell-contracts";
import { Folder, FolderOpen, GitBranch, MoreHorizontal, Plus } from "lucide-react";
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
  onSetTaskTitle?: (
    taskId: string,
    title: { kind: "user"; value: string } | { kind: "automatic" },
  ) => Promise<void>;
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
  onSetTaskTitle,
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
          {collapsed ? <Folder size={14} /> : <FolderOpen size={14} />}
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
      <div
        aria-hidden={collapsed}
        className={`project-task-group-rows ${collapsed ? "collapsed" : "expanded"}`}
        inert={collapsed}
      >
        {/* Keep rows mounted so closing can animate before the clipped region reaches zero height. */}
        <div className="project-task-group-rows-inner">
          {visibleRows.map((row) =>
            row.kind === "task" ? (
              <SidebarTaskRow
                key={`task:${row.task.task_id}`}
                activeTaskId={activeTaskId}
                onArchiveTask={onArchiveTask}
                onOpenTask={onOpenTask}
                onRestoreTask={onRestoreTask}
                onSetTaskTitle={onSetTaskTitle}
                showArchived={showArchived}
                task={row.task}
              />
            ) : (
              <SidebarNativeSessionRow
                key={`session:${row.session.agent_id ?? nativeSessionAgentId}:${row.session.session_id}`}
                nativeSessionAgentId={row.session.agent_id ?? nativeSessionAgentId}
                nativeSessionAgentName={row.session.agent_name ?? nativeSessionAgentName}
                nativeSessionsAdoptingSessionId={nativeSessionsAdoptingSessionId}
                onOpenNativeSession={onOpenNativeSession}
                session={row.session}
              />
            ),
          )}
          {hiddenCount > 0 || nativeSessionsHaveMore ? (
            <button
              className="project-task-more"
              onClick={() => onLoadMore(hiddenCount > 0 ? Math.min(pageSize, hiddenCount) : pageSize)}
              type="button"
            >
              Load more
            </button>
          ) : null}
        </div>
      </div>
    </section>
  );
}

function projectGroupCountSummary(taskCount: number, sessionCount: number) {
  const totalCount = taskCount + sessionCount;
  return totalCount ? `${totalCount} ${totalCount === 1 ? "task" : "tasks"}` : "";
}
