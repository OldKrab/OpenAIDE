import type { AgentListedSession } from "@openaide/app-shell-contracts";
import { Check, Folder, FolderOpen, GitBranch, MoreHorizontal, Pencil, Plus, Trash2, X } from "lucide-react";
import { useState } from "react";
import { SidebarNativeSessionRow } from "./SidebarNativeSessionRow";
import { PopupMenu } from "./Popup";
import { SidebarTaskRow } from "./SidebarTaskRow";
import {
  projectGroupRows,
  recentVisibleRows,
  type SidebarProjectGroup,
} from "./sidebarProjectModel";
import { useSidebarTaskPreview } from "./SidebarTaskPreview";
import { useRef } from "react";
import { taskForkMutationKey } from "../state/store";

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
  loading: boolean;
  canManageWorktrees: boolean;
  forkableAgentIds?: ReadonlySet<string>;
  onArchiveNativeSession: (session: AgentListedSession) => void;
  onArchiveTask: (taskId: string) => void;
  onForkNativeSession?: (session: AgentListedSession) => void;
  onForkTask?: (taskId: string) => void;
  onLoadMore: (visibleIncrement: number) => void;
  onManageWorktrees?: () => void;
  onNewTask: () => void;
  onRemoveProject?: () => void;
  onRenameProject?: (label: string) => Promise<void>;
  onOpenNativeSession: (session: AgentListedSession) => void;
  onOpenTask: (taskId: string) => void;
  onRestoreNativeSession: (session: AgentListedSession) => void;
  onRestoreTask: (taskId: string) => void;
  onSetTaskPinned?: (taskId: string, pinned: boolean) => Promise<void>;
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
  loading,
  canManageWorktrees,
  forkableAgentIds = new Set(),
  onArchiveNativeSession,
  onArchiveTask,
  onForkNativeSession,
  onForkTask,
  onLoadMore,
  onManageWorktrees,
  onNewTask,
  onRemoveProject,
  onRenameProject,
  onOpenNativeSession,
  onOpenTask,
  onRestoreNativeSession,
  onRestoreTask,
  onSetTaskPinned,
  onSetTaskTitle,
  onToggleCollapse,
  showArchived,
}: SidebarProjectTaskGroupProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [renameDraft, setRenameDraft] = useState(group.label);
  const [renameSaving, setRenameSaving] = useState(false);
  const [renameError, setRenameError] = useState<string>();
  const headerRef = useRef<HTMLDivElement>(null);
  const preview = useSidebarTaskPreview();
  const activeTask = group.tasks.find((task) => task.task_id === activeTaskId);
  const taskRows = projectGroupRows(group.tasks, []);
  const allRows = projectGroupRows(group.tasks, nativeSessions);
  const activeRow = activeTask
    ? allRows.find((row) => row.kind === "task" && row.task.task_id === activeTask.task_id)
    : undefined;
  const visibleRows = recentVisibleRows(allRows, maxTasks, activeRow);
  const hiddenCount = Math.max(0, allRows.length - visibleRows.length);
  const countSummary = loading
    ? "Loading…"
    : projectGroupCountSummary(taskRows.length, nativeSessions.length);
  const projectPreview = {
    kind: "project" as const,
    state: countSummary || "No tasks",
    title: group.label,
    workspaceLabel: group.workspaceRoot ?? "Path unavailable",
  };
  const saveRename = async () => {
    if (!onRenameProject || renameSaving || !renameDraft.trim()) return;
    setRenameSaving(true);
    setRenameError(undefined);
    try {
      await onRenameProject(renameDraft.trim());
      setRenaming(false);
    } catch (error) {
      setRenameError(error instanceof Error ? error.message : "Unable to rename Project.");
    } finally {
      setRenameSaving(false);
    }
  };

  return (
    <section className="project-task-group" aria-label={group.label}>
      <div
        className="project-task-group-header"
        onPointerLeave={(event) => preview?.leave(event.relatedTarget)}
        onPointerMove={() => !menuOpen && !renaming && headerRef.current && preview?.enter(projectPreview, headerRef.current)}
        ref={headerRef}
      >
        {renaming ? <form className="project-rename-form" onSubmit={(event) => { event.preventDefault(); void saveRename(); }}>
          <input aria-label={`Rename ${group.label}`} autoFocus disabled={renameSaving} maxLength={120} onChange={(event) => setRenameDraft(event.target.value)} value={renameDraft} />
          <button aria-label="Save Project name" disabled={renameSaving || !renameDraft.trim()} type="submit"><Check size={13} /></button>
          <button aria-label="Cancel Project rename" disabled={renameSaving} onClick={() => { setRenaming(false); setRenameError(undefined); }} type="button"><X size={13} /></button>
          {renameError ? <small role="alert">{renameError}</small> : null}
        </form> : <button
          aria-expanded={!collapsed}
          className="project-task-group-toggle"
          onFocus={() => !menuOpen && headerRef.current && preview?.enter(projectPreview, headerRef.current, true)}
          onClick={onToggleCollapse}
          type="button"
        >
          {collapsed ? <Folder size={14} /> : <FolderOpen size={14} />}
          <span>
            <strong>{group.label}</strong>
            {countSummary ? <small className="project-task-group-counts">{countSummary}</small> : null}
          </span>
        </button>}
        <div className="project-task-group-actions">
          <PopupMenu
            className="project-task-group-menu"
            label={`${group.label} actions`}
            onOpenChange={(open) => { if (open) preview?.dismiss(); setMenuOpen(open); }}
            open={menuOpen}
            trigger={(triggerProps) => (
              <button {...triggerProps} aria-label={`${group.label} actions`} type="button">
                <MoreHorizontal size={14} />
              </button>
            )}
          >
            <button onClick={() => { setMenuOpen(false); onNewTask(); }} role="menuitem" type="button"><Plus size={13} />New task</button>
            {canManageWorktrees && onManageWorktrees ? <button onClick={() => { setMenuOpen(false); onManageWorktrees(); }} role="menuitem" type="button"><GitBranch size={13} />Manage worktrees</button> : null}
            {onRenameProject ? <button onClick={() => { setMenuOpen(false); setRenameDraft(group.label); setRenameError(undefined); setRenaming(true); }} role="menuitem" type="button"><Pencil size={13} />Rename Project</button> : null}
            {onRemoveProject ? <button className="danger" onClick={() => { setMenuOpen(false); onRemoveProject(); }} role="menuitem" type="button"><Trash2 size={13} />Remove Project</button> : null}
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
                canFork={forkableAgentIds.has(row.task.agent_id) && !showArchived}
                forkMutation={nativeSessionMutations[taskForkMutationKey(row.task.task_id)]}
                onArchiveTask={onArchiveTask}
                onForkTask={onForkTask}
                onOpenTask={onOpenTask}
                onRestoreTask={onRestoreTask}
                onSetTaskPinned={onSetTaskPinned}
                onSetTaskTitle={onSetTaskTitle}
                showArchived={showArchived}
                task={row.task}
              />
            ) : (
              <SidebarNativeSessionRow
                archived={showArchived}
                canFork={forkableAgentIds.has(row.session.agent_id ?? nativeSessionAgentId) && !showArchived}
                key={`session:${row.session.agent_id ?? nativeSessionAgentId}:${row.session.session_id}`}
                mutation={nativeSessionMutations[
                  `${row.session.agent_id ?? nativeSessionAgentId}\u0000${row.session.session_id}`
                ]}
                nativeSessionAgentId={row.session.agent_id ?? nativeSessionAgentId}
                nativeSessionAgentName={row.session.agent_name ?? nativeSessionAgentName}
                nativeSessionsAdoptingSessionId={nativeSessionsAdoptingSessionId}
                onArchiveNativeSession={onArchiveNativeSession}
                onForkNativeSession={onForkNativeSession}
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
