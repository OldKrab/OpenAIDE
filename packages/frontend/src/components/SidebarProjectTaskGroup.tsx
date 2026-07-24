import type { AgentListedSession } from "@openaide/app-shell-contracts";
import { Folder, FolderOpen, GitBranch, MoreHorizontal, Plus } from "lucide-react";
import { useState } from "react";
import { SidebarNativeSessionRow } from "./SidebarNativeSessionRow";
import { PopupMenu } from "./Popup";
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
  nativeSessionMutations: import("../state/store").AppState["nativeSessionMutations"];
  nativeSessionsAdoptingSessionId?: string;
  nativeSessionsHaveMore: boolean;
  canManageWorktrees: boolean;
  onArchiveNativeSession: (session: AgentListedSession) => void;
  onArchiveTask: (taskId: string) => void;
  onLoadMore: (visibleIncrement: number) => void;
  onManageWorktrees?: () => void;
  onNewTask: () => void;
  onOpenNativeSession: (session: AgentListedSession) => void;
  onOpenTask: (taskId: string) => void;
  onRestoreNativeSession: (session: AgentListedSession) => void;
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
  nativeSessionMutations,
  nativeSessionsAdoptingSessionId,
  nativeSessionsHaveMore,
  canManageWorktrees,
  onArchiveNativeSession,
  onArchiveTask,
  onLoadMore,
  onManageWorktrees,
  onNewTask,
  onOpenNativeSession,
  onOpenTask,
  onRestoreNativeSession,
  onRestoreTask,
  onSetTaskTitle,
  onToggleCollapse,
  showArchived,
}: SidebarProjectTaskGroupProps) {
  const [menuOpen, setMenuOpen] = useState(false);
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
        <div className="project-task-group-actions">
          <PopupMenu
            className="project-task-group-menu"
            label={`${group.label} actions`}
            onOpenChange={setMenuOpen}
            open={menuOpen}
            trigger={(triggerProps) => (
              <button {...triggerProps} aria-label={`${group.label} actions`} type="button">
                <MoreHorizontal size={14} />
              </button>
            )}
          >
            <button onClick={() => { setMenuOpen(false); onNewTask(); }} role="menuitem" type="button"><Plus size={13} />New task</button>
            {canManageWorktrees && onManageWorktrees ? <button onClick={() => { setMenuOpen(false); onManageWorktrees(); }} role="menuitem" type="button"><GitBranch size={13} />Manage worktrees</button> : null}
          </PopupMenu>
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
                archived={showArchived}
                key={`session:${row.session.agent_id ?? nativeSessionAgentId}:${row.session.session_id}`}
                mutation={nativeSessionMutations[
                  `${row.session.agent_id ?? nativeSessionAgentId}\u0000${row.session.session_id}`
                ]}
                nativeSessionAgentId={row.session.agent_id ?? nativeSessionAgentId}
                nativeSessionAgentName={row.session.agent_name ?? nativeSessionAgentName}
                nativeSessionsAdoptingSessionId={nativeSessionsAdoptingSessionId}
                onArchiveNativeSession={onArchiveNativeSession}
                onOpenNativeSession={onOpenNativeSession}
                onRestoreNativeSession={onRestoreNativeSession}
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
