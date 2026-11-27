import type { AgentListedSession } from "@openaide/app-shell-contracts";
import { Check, Folder, FolderOpen, FolderTree, GitBranch, Monitor, MoreHorizontal, Pencil, Plus, SquareTerminal, Trash2, X } from "lucide-react";
import { useState } from "react";
import { SidebarNativeSessionRow } from "./SidebarNativeSessionRow";
import { PopupMenu } from "./Popup";
import { useBackNavigation } from "./useBackNavigation";
import { SidebarTaskRow } from "./SidebarTaskRow";
import { useSidebarTaskPreview } from "./SidebarTaskPreview";
import { useRef } from "react";
import { taskForkMutationKey, type NavigationGroupState, type NavigationRowState } from "../state/store";
import { useTerminalSurface } from "./terminals/terminalSurface";
import { currentFrontendShell } from "../services/frontendShell";
import type { ArchiveOlderTasksAction } from "./SidebarArchiveOlderMenu";

type SidebarProjectTaskGroupProps = {
  activeTaskId?: string;
  agentIcons?: import("./AgentIcon").AgentIconLookup;
  collapsed: boolean;
  /** The Project's rows as App Server sent them, in its order and window. */
  group: NavigationGroupState;
  workspaceRoot?: string;
  /** History rows App Server shows beside live rows, which no limit hides. */
  historyLimit: number;
  pageSize: number;
  nativeSessionAgentId: string;
  nativeSessionAgentName: string;
  nativeSessionMutations: import("../state/store").AppState["nativeSessionMutations"];
  nativeSessionsAdoptingSessionId?: string;
  canManageWorktrees: boolean;
  forkableAgentIds?: ReadonlySet<string>;
  deletableAgentIds?: ReadonlySet<string>;
  onDeleteSession?: import("../intents/sessionDeletionIntent").DeleteSessionAction;
  environmentLabel?: string;
  onArchiveNativeSession: (session: AgentListedSession) => void;
  onArchiveOlderNativeSessions?: ArchiveOlderTasksAction;
  onArchiveTask: (taskId: string) => void;
  onArchiveOlderTasks?: ArchiveOlderTasksAction;
  onForkNativeSession?: (session: AgentListedSession) => void;
  onForkTask?: (taskId: string) => void;
  /** Asks App Server for a larger history window; it reports the rows and counts back. */
  onLoadMore: (historyLimit: number) => void;
  onManageWorktrees?: () => void;
  onNewTask: () => void;
  /** Opens Project files as a page; absent where the App Shell cannot read files. */
  onBrowseFiles?: () => void;
  onRemoveProject?: () => void;
  onRenameProject?: (label: string) => Promise<void>;
  onOpenNativeSession: (session: AgentListedSession) => void;
  onOpenTask: (taskId: string) => void;
  onRestoreNativeSession: (session: AgentListedSession) => void;
  onSetNativeSessionPinned?: (session: AgentListedSession, pinned: boolean) => Promise<void>;
  onSetNativeSessionTitle?: (session: AgentListedSession, title: string) => Promise<void>;
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
  agentIcons,
  collapsed,
  group,
  workspaceRoot,
  historyLimit,
  pageSize,
  nativeSessionAgentId,
  nativeSessionAgentName,
  nativeSessionMutations,
  nativeSessionsAdoptingSessionId,
  canManageWorktrees,
  forkableAgentIds = new Set(),
  deletableAgentIds = new Set(),
  onDeleteSession,
  environmentLabel,
  onArchiveNativeSession,
  onArchiveOlderNativeSessions,
  onArchiveTask,
  onArchiveOlderTasks,
  onForkNativeSession,
  onForkTask,
  onLoadMore,
  onManageWorktrees,
  onNewTask,
  onBrowseFiles,
  onRemoveProject,
  onRenameProject,
  onOpenNativeSession,
  onOpenTask,
  onRestoreNativeSession,
  onSetNativeSessionPinned,
  onSetNativeSessionTitle,
  onRestoreTask,
  onSetTaskPinned,
  onSetTaskTitle,
  onToggleCollapse,
  showArchived,
}: SidebarProjectTaskGroupProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const terminalSurface = useTerminalSurface();
  // VS Code opens its own terminal at the Project root instead of the in-app page.
  const nativeTerminal = terminalSurface ? undefined : currentFrontendShell()?.nativeTerminal;
  const [renaming, setRenaming] = useState(false);
  const [renameDraft, setRenameDraft] = useState(group.label);
  const [renameSaving, setRenameSaving] = useState(false);
  const [renameError, setRenameError] = useState<string>();
  const cancelRename = () => {
    setRenaming(false);
    setRenameError(undefined);
  };
  // The platform Back action cancels an open rename, as its cancel button does, before the drawer closes.
  useBackNavigation(renaming && !renameSaving, cancelRename);
  const headerRef = useRef<HTMLDivElement>(null);
  const preview = useSidebarTaskPreview();
  const loading = group.discovery === "loading";
  const isLive = (row: NavigationRowState) =>
    row.live || (row.kind === "task" && row.task.task_id === activeTaskId);
  const { rows } = group;
  const liveRows = rows.filter(isLive);
  // More exists when App Server's window hides rows or the Project's sessions are not all read.
  const hasMore = group.hidden > 0 || group.undiscovered;
  const shownCount = group.total;
  const countSummary = loading
    ? "Loading…"
    : projectGroupCountSummary(shownCount);
  // A settled count renders its unit separately so phones can show the bare number.
  const countLabel = !loading && countSummary === plainCountSummary(shownCount)
    ? <>{shownCount}<span className="project-task-group-counts-unit">{` ${shownCount === 1 ? "task" : "tasks"}`}</span></>
    : countSummary;
  const projectPreview = {
    kind: "project" as const,
    state: countSummary || "No tasks",
    title: group.label,
    workspaceLabel: workspaceRoot ?? "Path unavailable",
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

  const renderRow = (row: NavigationRowState) => (
    row.kind === "task" ? (
      <SidebarTaskRow
        key={`task:${row.task.task_id}`}
        activeTaskId={activeTaskId}
        agentIcon={agentIcons?.[row.task.agent_id]}
        canFork={forkableAgentIds.has(row.task.agent_id) && !showArchived}
        onDeleteSession={deletableAgentIds.has(row.task.agent_id) ? onDeleteSession : undefined}
        forkMutation={nativeSessionMutations[taskForkMutationKey(row.task.task_id)]}
        onArchiveTask={onArchiveTask}
        onArchiveOlderTasks={onArchiveOlderTasks}
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
        agentIcon={agentIcons?.[row.session.agent_id ?? nativeSessionAgentId]}
        canFork={forkableAgentIds.has(row.session.agent_id ?? nativeSessionAgentId) && !showArchived}
        onDeleteSession={deletableAgentIds.has(row.session.agent_id ?? nativeSessionAgentId) ? onDeleteSession : undefined}
        key={`session:${row.session.agent_id ?? nativeSessionAgentId}:${row.session.session_id}`}
        mutation={nativeSessionMutations[
          `${row.session.agent_id ?? nativeSessionAgentId}\u0000${row.session.session_id}`
        ]}
        nativeSessionAgentId={row.session.agent_id ?? nativeSessionAgentId}
        nativeSessionAgentName={row.session.agent_name ?? nativeSessionAgentName}
        nativeSessionsAdoptingSessionId={nativeSessionsAdoptingSessionId}
        onArchiveNativeSession={onArchiveNativeSession}
        onArchiveOlderNativeSessions={onArchiveOlderNativeSessions}
        onForkNativeSession={onForkNativeSession}
        onOpenNativeSession={onOpenNativeSession}
        onRestoreNativeSession={onRestoreNativeSession}
        onSetNativeSessionPinned={onSetNativeSessionPinned}
        onSetNativeSessionTitle={onSetNativeSessionTitle}
        session={row.session}
      />
    )
  );

  return (
    <section className="project-task-group" aria-label={group.label}>
      <div
        className="project-task-group-header"
        onPointerLeave={(event) => preview?.leave(event.relatedTarget)}
        onPointerMove={() => !menuOpen && !renaming && headerRef.current && preview?.enter(projectPreview, headerRef.current)}
        ref={headerRef}
      >
        {renaming ? <form className="project-rename-form" onSubmit={(event) => { event.preventDefault(); void saveRename(); }}>
          <input aria-label={`Rename ${group.label}`} autoFocus disabled={renameSaving} maxLength={120} onChange={(event) => {
            setRenameDraft(event.target.value);
            setRenameError(undefined);
          }} value={renameDraft} />
          <button aria-label="Save Project name" disabled={renameSaving || !renameDraft.trim()} type="submit"><Check size={13} /></button>
          <button aria-label="Cancel Project rename" disabled={renameSaving} onClick={cancelRename} type="button"><X size={13} /></button>
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
            {countSummary ? <small className="project-task-group-counts">{countLabel}</small> : null}
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
            {environmentLabel ? (
              <div className="project-task-group-environment" role="none">
                <Monitor aria-hidden="true" size={13} />
                <span><small>Environment</small><strong>{environmentLabel}</strong></span>
              </div>
            ) : null}
            <button onClick={() => { setMenuOpen(false); onNewTask(); }} role="menuitem" type="button"><Plus size={13} />New task</button>
            {(terminalSurface || nativeTerminal) && workspaceRoot ? (
              <button
                onClick={() => {
                  setMenuOpen(false);
                  if (terminalSurface) terminalSurface.openPage({ projectId: group.projectId });
                  else nativeTerminal?.open(workspaceRoot!);
                }}
                role="menuitem"
                type="button"
              >
                <SquareTerminal size={13} />Open terminal
              </button>
            ) : null}
            {onBrowseFiles ? <button onClick={() => { setMenuOpen(false); onBrowseFiles(); }} role="menuitem" type="button"><FolderTree size={13} />Browse files</button> : null}
            {canManageWorktrees && onManageWorktrees ? <button onClick={() => { setMenuOpen(false); onManageWorktrees(); }} role="menuitem" type="button"><GitBranch size={13} />Manage worktrees</button> : null}
            {onRenameProject ? <button onClick={() => { setMenuOpen(false); setRenameDraft(group.label); setRenameError(undefined); setRenaming(true); }} role="menuitem" type="button"><Pencil size={13} />Rename Project</button> : null}
            {onRemoveProject ? <button className="danger" onClick={() => { setMenuOpen(false); onRemoveProject(); }} role="menuitem" type="button"><Trash2 size={13} />Remove Project</button> : null}
          </PopupMenu>
        </div>
      </div>
      {collapsed && liveRows.length > 0 ? (
        <div className="project-task-group-live">{liveRows.map(renderRow)}</div>
      ) : null}
      <div
        aria-hidden={collapsed}
        className={`project-task-group-rows ${collapsed ? "collapsed" : "expanded"}`}
        inert={collapsed}
      >
        {/* Keep rows mounted so closing can animate before the clipped region reaches zero height. */}
        <div className="project-task-group-rows-inner">
          {(collapsed ? rows.filter((row) => !isLive(row)) : rows).map(renderRow)}
          {hasMore ? (
            <button
              aria-busy={loading || undefined}
              className="project-task-more"
              disabled={loading}
              onClick={() => {
                const increment = group.hidden > 0 ? Math.min(pageSize, group.hidden) : pageSize;
                onLoadMore(historyLimit + increment);
              }}
              type="button"
            >
              {loadMoreLabel(group.hidden, pageSize)}
            </button>
          ) : null}
        </div>
      </div>
    </section>
  );
}

/** Names the rows a press reveals when they are already loaded; further Agent pages have no known count. */
function loadMoreLabel(hiddenCount: number, pageSize: number) {
  return hiddenCount > 0 ? `Show ${Math.min(pageSize, hiddenCount)} more` : "Show more";
}

function projectGroupCountSummary(totalCount: number) {
  return totalCount ? plainCountSummary(totalCount) : "";
}

function plainCountSummary(totalCount: number) {
  return `${totalCount} ${totalCount === 1 ? "task" : "tasks"}`;
}
