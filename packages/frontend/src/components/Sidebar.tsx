import { memo, useRef, useState } from "react";
import { Archive, ArrowLeft, Plus, RefreshCcw, Search, Settings } from "lucide-react";
import type { AgentListedSession, TaskSummary } from "@openaide/app-shell-contracts";
import type { ProjectOption } from "../state/composerOptions";
import type { AppState } from "../state/store";
import {
  initialTaskNavigationRowsPerProject,
  TASK_NAVIGATION_PAGE_SIZE,
} from "../state/taskNavigationPolicy";
import { SidebarNativeSessionRow } from "./SidebarNativeSessionRow";
import { SidebarProjectTaskGroup } from "./SidebarProjectTaskGroup";
import { SidebarTaskRow } from "./SidebarTaskRow";
import { groupedTasks, projectGroupRows, recentVisibleGroups, taskMatchesSearch } from "./sidebarProjectModel";
import { sidebarViewModel } from "./sidebarViewModel";
import { SidebarTaskPreviewProvider } from "./SidebarTaskPreview";
import { useScrollOverflow } from "./useScrollOverflow";

type SidebarProps = {
  activeTaskId?: string;
  nativeSessions: AppState["newTask"]["nativeSessions"];
  nativeSessionAgentId: string;
  nativeSessionAgentName: string;
  nativeSessionProjectId?: string;
  onLoadNativeSessions: (cursor?: string, projectId?: string, targetRowCount?: number) => void;
  onManageWorktrees?: (projectId: string) => void;
  onNewTask: (projectId?: string) => void;
  onOpenNativeSession: (session: AgentListedSession) => void;
  onOpenTask: (taskId: string) => void;
  onRecoverNativeSessions?: (kind: NonNullable<AppState["newTask"]["nativeSessions"]["recoveryKind"]>) => void;
  onArchiveTask: (taskId: string) => void;
  onRestoreTask: (taskId: string) => void;
  onSetTaskTitle?: (
    taskId: string,
    title: { kind: "user"; value: string } | { kind: "automatic" },
  ) => Promise<void>;
  onSearchChange: (query: string) => void;
  onSettings: () => void;
  onToggleArchived: () => void;
  searchQuery: string;
  settingsActive?: boolean;
  showArchived: boolean;
  taskListError?: string;
  tasks: TaskSummary[];
  groupByProject?: boolean;
  hiddenFromAccessibility?: boolean;
  modal?: boolean;
  projects?: ProjectOption[];
  maxTasksPerProject?: number;
  maxVisibleProjects?: number;
  loadingTasks?: boolean;
  showNativeSessions?: boolean;
};

export const DEFAULT_MAX_TASKS_PER_PROJECT = 20;

export const Sidebar = memo(function Sidebar({
  activeTaskId,
  nativeSessions,
  nativeSessionAgentId,
  nativeSessionAgentName,
  nativeSessionProjectId,
  onLoadNativeSessions,
  onManageWorktrees,
  onNewTask,
  onOpenNativeSession,
  onOpenTask,
  onRecoverNativeSessions,
  onArchiveTask,
  onRestoreTask,
  onSetTaskTitle,
  onSearchChange,
  onSettings,
  onToggleArchived,
  searchQuery,
  settingsActive = false,
  showArchived,
  taskListError,
  tasks,
  groupByProject = false,
  hiddenFromAccessibility = false,
  modal = false,
  projects = [],
  maxTasksPerProject,
  maxVisibleProjects = 5,
  loadingTasks = false,
  showNativeSessions = true,
}: SidebarProps) {
  const taskListRef = useRef<HTMLDivElement>(null);
  const taskListOverflow = useScrollOverflow(taskListRef, showArchived);
  const [collapsedProjectKeys, setCollapsedProjectKeys] = useState<Set<string>>(() => new Set());
  const [projectRowLimits, setProjectRowLimits] = useState<Map<string, number>>(() => new Map());
  const [visibleProjectLimit, setVisibleProjectLimit] = useState(maxVisibleProjects);
  const displayedNativeSessions = showNativeSessions
    ? nativeSessions
    : { adoptingSessionId: undefined, error: undefined, items: [], loaded: true, loading: false, nextCursor: undefined };
  const viewModel = sidebarViewModel({
    loadingTasks,
    nativeSessionAgentName,
    nativeSessions: displayedNativeSessions,
    searchQuery,
    showArchived,
    taskCount: tasks.length,
  });
  const flatRows = projectGroupRows(
    tasks,
    !showArchived && showNativeSessions ? viewModel.visibleNativeSessions : [],
  );
  const groupSearchQuery = searchQuery.trim().toLowerCase();
  const hasSearchQuery = groupSearchQuery.length > 0;
  const activeTask = activeTaskId ? tasks.find((task) => task.task_id === activeTaskId) : undefined;
  const activeTaskShownOutsideSearch =
    hasSearchQuery && activeTask !== undefined && !taskMatchesSearch(activeTask, groupSearchQuery);
  const groups = groupedTasks(tasks, projects, {
    includeProjectId: nativeSessionProjectId,
    includedProjectSessions:
      !showArchived && showNativeSessions ? viewModel.visibleNativeSessions : [],
  }).filter((group) =>
    !groupSearchQuery ||
    group.tasks.length > 0 ||
    group.label.toLowerCase().includes(groupSearchQuery) ||
    group.nativeSessions.length > 0,
  );
  const initialProjectRowLimit = maxTasksPerProject
    ?? initialTaskNavigationRowsPerProject(groups.length);
  const activeProjectKey = activeTask?.project_id;
  const visibleGroups = groupByProject
    ? recentVisibleGroups(groups, Math.max(1, visibleProjectLimit), activeProjectKey)
    : [];
  const hiddenProjectCount = groupByProject ? Math.max(0, groups.length - visibleGroups.length) : 0;
  const selectedSessionProjectCollapsed =
    groupByProject &&
    !groupSearchQuery &&
    nativeSessionProjectId !== undefined &&
    collapsedProjectKeys.has(nativeSessionProjectId);
  const showEmptyState = !taskListError && (groupByProject ? groups.length === 0 : viewModel.visibleCount === 0);
  const showSessionRefresh = !showArchived && showNativeSessions;

  return (
    <aside
      className={`sidebar ${showArchived ? "archive-sidebar" : ""}`}
      aria-hidden={hiddenFromAccessibility ? true : undefined}
      aria-label="Task navigation"
      aria-modal={modal ? true : undefined}
      inert={hiddenFromAccessibility ? true : undefined}
      role={modal ? "dialog" : undefined}
    >
      {showArchived ? (
        <div className="archive-section-head">
          <button aria-label="Back to tasks" onClick={onToggleArchived} type="button"><ArrowLeft size={15} /></button>
          <Archive size={15} />
          <span><strong>Archive</strong><small>Read-only tasks</small></span>
        </div>
      ) : null}
      <div className={`sidebar-actions ${showArchived ? "archive-actions" : ""}`}>
        {!showArchived ? <button type="button" onClick={() => onNewTask()}>
          <Plus size={15} />
          New task
        </button> : null}
        <label className="sidebar-search">
          <Search size={15} />
          <input
            aria-label={showArchived ? "Search archive" : "Search tasks"}
            onChange={(event) => onSearchChange(event.target.value)}
            placeholder={showArchived ? "Search archive" : "Search"}
            value={searchQuery}
          />
        </label>
      </div>
      {!showArchived ? <div className="task-section-head">
        <span className="task-section-title">Tasks</span>
        {showSessionRefresh ? (
          <span className="task-section-tools">
            <button
              aria-label="Refresh tasks"
              className={`task-section-refresh ${nativeSessions.loading ? "refreshing" : ""}`}
              disabled={nativeSessions.loading || nativeSessions.adoptingSessionId !== undefined}
              onClick={() => onLoadNativeSessions()}
              title="Refresh tasks"
              type="button"
            >
              <RefreshCcw size={13} />
            </button>
            {nativeSessions.loading ? <small>Refreshing tasks</small> : null}
          </span>
        ) : null}
        <button className="archive-navigation" onClick={onToggleArchived} type="button"><Archive size={13} />Archive</button>
      </div> : null}
      <SidebarTaskPreviewProvider><div className="task-list-shell" data-more-below={String(taskListOverflow.moreBelow)}><div
        className="task-list"
        role="list"
        aria-label={showArchived ? "Archived tasks" : "Tasks"}
        onScroll={taskListOverflow.onScroll}
        ref={taskListRef}
      >
        {taskListError ? <p className="empty-list">{taskListError}</p> : null}
        {showEmptyState
          ? <p className="empty-list">{viewModel.emptyMessage}</p>
          : null}
        {activeTaskShownOutsideSearch ? (
          <p className="search-context-note">Selected task is shown outside the search results.</p>
        ) : null}
        {!showArchived && showNativeSessions && nativeSessions.error ? (
          <div className="native-session-recovery" role="status">
            <span>{nativeSessions.error}</span>
            {nativeSessions.recoveryKind && onRecoverNativeSessions ? (
              <button type="button" onClick={() => onRecoverNativeSessions(nativeSessions.recoveryKind!)}>
                {nativeSessions.recoveryKind === "authRequired"
                  ? "Sign in"
                  : nativeSessions.recoveryKind === "launchFailed" ? "Try again" : "Set up Codex"}
              </button>
            ) : null}
          </div>
        ) : null}
        {groupByProject
          ? visibleGroups.map((group) => (
              <SidebarProjectTaskGroup
                activeTaskId={activeTaskId}
                collapsed={groupSearchQuery ? false : collapsedProjectKeys.has(group.key)}
                group={group}
                key={group.key}
                maxTasks={projectRowLimits.get(group.key) ?? initialProjectRowLimit}
                pageSize={TASK_NAVIGATION_PAGE_SIZE}
                nativeSessionAgentId={nativeSessionAgentId}
                nativeSessionAgentName={nativeSessionAgentName}
                nativeSessions={group.nativeSessions}
                nativeSessionsAdoptingSessionId={nativeSessions.adoptingSessionId}
                nativeSessionsHaveMore={
                  !showArchived && nativeSessions.hasMoreProjectIds?.includes(group.key) === true
                }
                canManageWorktrees={Boolean(projects.find((project) => project.projectId === group.key)?.worktreeRepositoryId)}
                onArchiveTask={onArchiveTask}
                onLoadMore={(visibleIncrement) =>
                  {
                    const nextLimit = (projectRowLimits.get(group.key) ?? initialProjectRowLimit) + visibleIncrement;
                    setProjectRowLimits((current) => {
                    const next = new Map(current);
                    next.set(group.key, nextLimit);
                    return next;
                    });
                    onLoadNativeSessions(undefined, group.key, nextLimit);
                  }
                }
                onManageWorktrees={onManageWorktrees ? () => onManageWorktrees(group.key) : undefined}
                onNewTask={() => onNewTask(group.key)}
                onOpenNativeSession={onOpenNativeSession}
                onOpenTask={onOpenTask}
                onRestoreTask={onRestoreTask}
                onSetTaskTitle={onSetTaskTitle}
                onToggleCollapse={() =>
                  setCollapsedProjectKeys((current) => {
                    const next = new Set(current);
                    if (next.has(group.key)) {
                      next.delete(group.key);
                      setProjectRowLimits((limits) => {
                        const reset = new Map(limits);
                        reset.delete(group.key);
                        return reset;
                      });
                    } else {
                      next.add(group.key);
                    }
                    return next;
                  })
                }
                showArchived={showArchived}
              />
            ))
          : flatRows.map((row) =>
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
                  nativeSessionsAdoptingSessionId={nativeSessions.adoptingSessionId}
                  onOpenNativeSession={onOpenNativeSession}
                  session={row.session}
                />
              ),
            )}
        {!groupByProject && !showArchived && showNativeSessions && nativeSessions.nextCursor && !selectedSessionProjectCollapsed ? (
          <button
            className="session-more"
            disabled={nativeSessions.adoptingSessionId !== undefined || nativeSessions.loading}
            onClick={() => {
              if (nativeSessions.nextCursor) {
                onLoadNativeSessions(nativeSessions.nextCursor);
              }
            }}
            type="button"
          >
            {nativeSessions.loading
              ? hasSearchQuery ? "Searching tasks" : "Loading tasks"
              : hasSearchQuery ? "Search more tasks" : "Load more tasks"}
          </button>
        ) : null}
        {groupByProject && hiddenProjectCount > 0 ? (
          <button
            className="project-more"
            onClick={() => setVisibleProjectLimit((current) => current + maxVisibleProjects)}
            type="button"
          >
            Show {Math.min(maxVisibleProjects, hiddenProjectCount)} more workspaces
          </button>
        ) : null}
      </div></div></SidebarTaskPreviewProvider>
      <div className="sidebar-footer">
        <button
          aria-current={settingsActive ? "page" : undefined}
          className={`settings-button ${settingsActive ? "selected" : ""}`}
          type="button"
          onClick={onSettings}
        >
          <Settings size={15} />
          Settings
        </button>
      </div>
    </aside>
  );
}, sameSidebarDataProps);

function sameSidebarDataProps(prev: SidebarProps, next: SidebarProps) {
  return prev.activeTaskId === next.activeTaskId &&
    prev.nativeSessions === next.nativeSessions &&
    prev.nativeSessionAgentId === next.nativeSessionAgentId &&
    prev.nativeSessionAgentName === next.nativeSessionAgentName &&
    prev.nativeSessionProjectId === next.nativeSessionProjectId &&
    prev.searchQuery === next.searchQuery &&
    prev.settingsActive === next.settingsActive &&
    prev.showArchived === next.showArchived &&
    prev.taskListError === next.taskListError &&
    prev.tasks === next.tasks &&
    prev.groupByProject === next.groupByProject &&
    prev.hiddenFromAccessibility === next.hiddenFromAccessibility &&
    prev.modal === next.modal &&
    prev.projects === next.projects &&
    prev.maxTasksPerProject === next.maxTasksPerProject &&
    prev.maxVisibleProjects === next.maxVisibleProjects &&
    prev.loadingTasks === next.loadingTasks &&
    prev.showNativeSessions === next.showNativeSessions;
}
