import { memo, useEffect, useRef, useState } from "react";
import { Archive, ArrowLeft, FolderPlus, FolderSync, FolderX, Plus, RefreshCcw, RotateCcw, Search, Settings } from "lucide-react";
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
import { WorkspaceSetupPrompt } from "./WorkspaceSetupPrompt";
import { NewWorkspacePicker } from "./NewWorkspacePicker";
import type { WorkspaceBrowserCallbacks } from "./appControllerCallbackTypes";
import { currentFrontendShell } from "../services/frontendShell";

type SidebarProps = {
  activeTaskId?: string;
  nativeSessions: AppState["newTask"]["nativeSessions"];
  nativeSessionMutations?: AppState["nativeSessionMutations"];
  nativeSessionAgentId: string;
  nativeSessionAgentName: string;
  nativeSessionProjectId?: string;
  onArchiveNativeSession: (session: AgentListedSession) => void;
  onLoadNativeSessions: (cursor?: string, projectId?: string, targetRowCount?: number) => void;
  onManageWorktrees?: (projectId: string) => void;
  onRegisterProject?: (root: string, label?: string) => Promise<void>;
  onReconnectProject?: (projectId: string, root: string) => Promise<void>;
  onRemoveProject?: (projectId: string) => Promise<void>;
  onRenameProject?: (projectId: string, label: string) => Promise<void>;
  onNewTask: (projectId?: string) => void;
  onOpenNativeSession: (session: AgentListedSession) => void;
  onOpenWorkspaceFolder?: () => void;
  onOpenTask: (taskId: string) => void;
  onRecoverNativeSessions?: (kind: NonNullable<AppState["newTask"]["nativeSessions"]["recoveryKind"]>) => void;
  onArchiveTask: (taskId: string) => void;
  onRestoreNativeSession: (session: AgentListedSession) => void;
  onRestoreTask: (taskId: string) => void;
  onSetTaskPinned?: (taskId: string, pinned: boolean) => Promise<void>;
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
  removedProjects?: ProjectOption[];
  workspaceBrowser?: WorkspaceBrowserCallbacks;
  maxTasksPerProject?: number;
  maxVisibleProjects?: number;
  loadingTasks?: boolean;
  showNativeSessions?: boolean;
};

export const Sidebar = memo(function Sidebar({
  activeTaskId,
  nativeSessions,
  nativeSessionMutations = {},
  nativeSessionAgentId,
  nativeSessionAgentName,
  nativeSessionProjectId,
  onArchiveNativeSession,
  onLoadNativeSessions,
  onManageWorktrees,
  onRegisterProject,
  onReconnectProject,
  onRemoveProject,
  onRenameProject,
  onNewTask,
  onOpenNativeSession,
  onOpenWorkspaceFolder,
  onOpenTask,
  onRecoverNativeSessions,
  onArchiveTask,
  onRestoreNativeSession,
  onRestoreTask,
  onSetTaskPinned,
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
  removedProjects = [],
  workspaceBrowser,
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
  const [showRemovedProjects, setShowRemovedProjects] = useState(false);
  const [projectFolderIntent, setProjectFolderIntent] = useState<
    { kind: "register" } | { kind: "reconnect"; projectId: string } | undefined
  >();
  const [projectMutationError, setProjectMutationError] = useState<string>();
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
    showNativeSessions ? viewModel.visibleNativeSessions : [],
  );
  const groupSearchQuery = searchQuery.trim().toLowerCase();
  const hasSearchQuery = groupSearchQuery.length > 0;
  const activeTask = activeTaskId ? tasks.find((task) => task.task_id === activeTaskId) : undefined;
  const activeTaskShownOutsideSearch =
    hasSearchQuery && activeTask !== undefined && !taskMatchesSearch(activeTask, groupSearchQuery);
  const groups = groupedTasks(tasks, projects, {
    includeProjectId: nativeSessionProjectId,
    includedProjectSessions:
      showNativeSessions ? viewModel.visibleNativeSessions : [],
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
  const showWorkspaceSetup = !showArchived && !showRemovedProjects && onOpenWorkspaceFolder !== undefined;
  const showSessionRefresh = !showArchived && showNativeSessions && !showWorkspaceSetup;
  const chooseProjectRoot = async (intent: NonNullable<typeof projectFolderIntent>) => {
    const nativePicker = currentFrontendShell()?.projects;
    if (nativePicker) {
      const nativeRoot = await nativePicker.pickRoot();
      if (nativeRoot) await applyProjectRoot(intent, nativeRoot);
      return;
    }
    if (workspaceBrowser) setProjectFolderIntent(intent);
  };
  const applyProjectRoot = async (
    intent: NonNullable<typeof projectFolderIntent>,
    root: { path: string; label: string },
  ) => {
    setProjectMutationError(undefined);
    try {
      if (intent.kind === "register") await onRegisterProject?.(root.path, root.label);
      else await onReconnectProject?.(intent.projectId, root.path);
      setProjectFolderIntent(undefined);
    } catch (error) {
      setProjectMutationError(error instanceof Error ? error.message : "Unable to update Project.");
    }
  };
  useEffect(() => currentFrontendShell()?.projects?.subscribeAddRequest?.((root) => {
    setProjectMutationError(undefined);
    void onRegisterProject?.(root.path, root.label).catch((error: unknown) => {
      setProjectMutationError(error instanceof Error ? error.message : "Unable to add Project.");
    });
  }), [onRegisterProject]);
  useEffect(() => {
    if (!projectFolderIntent) return undefined;
    const closeOnEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") setProjectFolderIntent(undefined);
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [projectFolderIntent]);

  return (
    <aside
      className={`sidebar ${showArchived ? "archive-sidebar" : ""} ${showRemovedProjects ? "removed-projects-sidebar" : ""}`}
      aria-hidden={hiddenFromAccessibility ? true : undefined}
      aria-label="Task navigation"
      aria-modal={modal ? true : undefined}
      inert={hiddenFromAccessibility ? true : undefined}
      role={modal ? "dialog" : undefined}
    >
      {showRemovedProjects ? (
        <div className="archive-section-head">
          <button aria-label="Back to tasks" onClick={() => setShowRemovedProjects(false)} type="button"><ArrowLeft size={15} /></button>
          <FolderX size={15} />
          <span><strong>Removed Projects</strong><small>History is kept</small></span>
        </div>
      ) : showArchived ? (
        <div className="archive-section-head">
          <button aria-label="Back to tasks" onClick={onToggleArchived} type="button"><ArrowLeft size={15} /></button>
          <Archive size={15} />
          <span><strong>Archive</strong><small>Tasks and Native Sessions</small></span>
        </div>
      ) : null}
      <div className={`sidebar-actions ${showArchived || showRemovedProjects ? "archive-actions" : ""}`}>
        {!showArchived && !showRemovedProjects ? <button type="button" onClick={() => onNewTask()}>
          <Plus size={15} />
          New task
        </button> : null}
        {!showRemovedProjects ? <label className="sidebar-search">
          <Search size={15} />
          <input
            aria-label={showArchived ? "Search archive" : "Search tasks"}
            onChange={(event) => onSearchChange(event.target.value)}
            placeholder={showArchived ? "Search archive" : "Search"}
            value={searchQuery}
          />
        </label> : null}
      </div>
      {!showArchived && !showRemovedProjects ? <div className="task-section-head">
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
      {!showArchived && !showRemovedProjects && groupByProject && onRegisterProject ? <div className="project-section-head">
        <span>Projects</span>
        <button aria-label="Add Project" onClick={() => void chooseProjectRoot({ kind: "register" })} title="Add Project" type="button"><FolderPlus size={13} />Add</button>
        {removedProjects.length > 0 ? <button onClick={() => setShowRemovedProjects(true)} type="button"><FolderX size={13} />Removed</button> : null}
      </div> : null}
      {!showArchived && !showRemovedProjects && projectMutationError ? <p className="project-sidebar-error" role="status">{projectMutationError}</p> : null}
      <SidebarTaskPreviewProvider><div className="task-list-shell" data-more-below={String(taskListOverflow.moreBelow)}><div
        className="task-list"
        role="list"
        aria-label={showArchived ? "Archived tasks" : "Tasks"}
        onScroll={taskListOverflow.onScroll}
        ref={taskListRef}
      >
        {showRemovedProjects ? <div className="removed-project-list">
          {projectMutationError ? <p className="project-folder-dialog-error" role="status">{projectMutationError}</p> : null}
          {removedProjects.length === 0 ? <p className="empty-list">No removed Projects.</p> : removedProjects.map((project) => (
            <article className="removed-project-row" key={project.projectId}>
              <span><strong>{project.label}</strong><small>{project.workspaceRoot}</small></span>
              <div>
                {project.workspaceRoot && project.available !== false ? <button onClick={() => {
                  setProjectMutationError(undefined);
                  void onReconnectProject?.(project.projectId, project.workspaceRoot!).catch((error: unknown) => {
                    setProjectMutationError(error instanceof Error ? error.message : "Unable to restore Project.");
                  });
                }} title="Restore using the saved folder" type="button"><RotateCcw size={13} />Restore</button> : null}
                <button onClick={() => void chooseProjectRoot({ kind: "reconnect", projectId: project.projectId })} type="button"><FolderSync size={13} />Choose folder</button>
              </div>
            </article>
          ))}
        </div> : null}
        {!showRemovedProjects && !showWorkspaceSetup && taskListError ? <p className="empty-list">{taskListError}</p> : null}
        {showWorkspaceSetup
          ? <WorkspaceSetupPrompt compact onOpenFolder={onOpenWorkspaceFolder} />
          : null}
        {!showRemovedProjects && !showWorkspaceSetup && showEmptyState
          ? <p className="empty-list">{viewModel.emptyMessage}</p>
          : null}
        {!showRemovedProjects && !showWorkspaceSetup && activeTaskShownOutsideSearch ? (
          <p className="search-context-note">Selected task is shown outside the search results.</p>
        ) : null}
        {!showRemovedProjects && !showWorkspaceSetup && !showArchived && showNativeSessions && nativeSessions.error ? (
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
        {!showRemovedProjects && !showWorkspaceSetup && (groupByProject
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
                nativeSessionMutations={nativeSessionMutations}
                nativeSessionsAdoptingSessionId={nativeSessions.adoptingSessionId}
                nativeSessionsHaveMore={
                  !showArchived && nativeSessions.hasMoreProjectIds?.includes(group.key) === true
                }
                canManageWorktrees={Boolean(projects.find((project) => project.projectId === group.key)?.worktreeRepositoryId)}
                onArchiveNativeSession={onArchiveNativeSession}
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
                onReconnectProject={onReconnectProject ? () => void chooseProjectRoot({ kind: "reconnect", projectId: group.key }) : undefined}
                onRemoveProject={onRemoveProject ? () => onRemoveProject(group.key) : undefined}
                onRenameProject={onRenameProject ? (label) => onRenameProject(group.key, label) : undefined}
                onNewTask={() => onNewTask(group.key)}
                onOpenNativeSession={onOpenNativeSession}
                onOpenTask={onOpenTask}
                onRestoreNativeSession={onRestoreNativeSession}
                onRestoreTask={onRestoreTask}
                onSetTaskPinned={onSetTaskPinned}
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
                  onSetTaskPinned={onSetTaskPinned}
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
                  nativeSessionsAdoptingSessionId={nativeSessions.adoptingSessionId}
                  onArchiveNativeSession={onArchiveNativeSession}
                  onOpenNativeSession={onOpenNativeSession}
                  onRestoreNativeSession={onRestoreNativeSession}
                  session={row.session}
                />
              ),
            ))}
        {!showRemovedProjects && !showWorkspaceSetup && !groupByProject && !showArchived && showNativeSessions && nativeSessions.nextCursor && !selectedSessionProjectCollapsed ? (
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
        {!showRemovedProjects && !showWorkspaceSetup && groupByProject && hiddenProjectCount > 0 ? (
          <button
            className="project-more"
            onClick={() => setVisibleProjectLimit((current) => current + maxVisibleProjects)}
            type="button"
          >
            Show {Math.min(maxVisibleProjects, hiddenProjectCount)} more workspaces
          </button>
        ) : null}
      </div></div></SidebarTaskPreviewProvider>
      {projectFolderIntent && workspaceBrowser ? <div className="project-folder-dialog-backdrop" role="presentation">
        <section aria-label={projectFolderIntent.kind === "register" ? "Add Project" : "Reconnect Project"} aria-modal="true" className="project-folder-dialog" role="dialog">
          <header><strong>{projectFolderIntent.kind === "register" ? "Add Project" : "Reconnect Project"}</strong><button aria-label="Close" autoFocus onClick={() => setProjectFolderIntent(undefined)} type="button">×</button></header>
          {projectMutationError ? <p className="project-folder-dialog-error" role="status">{projectMutationError}</p> : null}
          <NewWorkspacePicker browser={workspaceBrowser} onSelect={(root) => void applyProjectRoot(projectFolderIntent, root)} />
        </section>
      </div> : null}
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
    prev.nativeSessionMutations === next.nativeSessionMutations &&
    prev.nativeSessionAgentId === next.nativeSessionAgentId &&
    prev.nativeSessionAgentName === next.nativeSessionAgentName &&
    prev.nativeSessionProjectId === next.nativeSessionProjectId &&
    prev.onOpenWorkspaceFolder === next.onOpenWorkspaceFolder &&
    prev.searchQuery === next.searchQuery &&
    prev.settingsActive === next.settingsActive &&
    prev.showArchived === next.showArchived &&
    prev.taskListError === next.taskListError &&
    prev.tasks === next.tasks &&
    prev.groupByProject === next.groupByProject &&
    prev.hiddenFromAccessibility === next.hiddenFromAccessibility &&
    prev.modal === next.modal &&
    prev.projects === next.projects &&
    prev.removedProjects === next.removedProjects &&
    prev.workspaceBrowser === next.workspaceBrowser &&
    prev.maxTasksPerProject === next.maxTasksPerProject &&
    prev.maxVisibleProjects === next.maxVisibleProjects &&
    prev.loadingTasks === next.loadingTasks &&
    prev.showNativeSessions === next.showNativeSessions;
}
