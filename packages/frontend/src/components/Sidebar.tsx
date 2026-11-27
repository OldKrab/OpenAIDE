import { memo, useRef, useState } from "react";
import { Archive, ArrowLeft, FolderPlus, LoaderCircle, Plus, RefreshCcw, Search, Settings, X } from "lucide-react";
import type { AgentListedSession } from "@openaide/app-shell-contracts";
import type { TaskArchiveOlderCutoff, TaskArchiveOlderResult } from "@openaide/app-server-client";
import type { ProjectOption } from "../state/composerOptions";
import type { AppState } from "../state/store";
import { taskNavigationPageSize } from "../state/taskNavigationPolicy";
import { SidebarProjectTaskGroup } from "./SidebarProjectTaskGroup";
import { SidebarTaskPreviewProvider } from "./SidebarTaskPreview";
import { useScrollOverflow } from "./useScrollOverflow";
import { usePullToRefresh } from "./usePullToRefresh";
import { WorkspaceSetupPrompt } from "./WorkspaceSetupPrompt";
import { AgentDisableDialog } from "./AgentDisableDialog";
import { useAgentDisableConfirmation } from "./useAgentDisableConfirmation";
import { ComputerSwitcher, type ComputerLink } from "./ComputerSwitcher";
import type { ComputersSnapshot } from "../services/computers";
import type { ComputerSwitch } from "../services/computerSwitch";
import type { AgentDisableOutcome } from "../intents/agentSettingsIntents";
import { agentIntegrationInstallingLabel } from "./agentActivityPresentation";
import { SidebarSearchResults } from "./SidebarSearchResults";
import type { RunNavigationSearch } from "./useNavigationSearch";

type SidebarProps = {
  activeTaskId?: string;
  agentIcons?: import("./AgentIcon").AgentIconLookup;
  nativeSessions: AppState["newTask"]["nativeSessions"];
  nativeSessionMutations?: AppState["nativeSessionMutations"];
  nativeSessionAgentId: string;
  nativeSessionAgentName: string;
  forkableAgentIds?: ReadonlySet<string>;
  deletableAgentIds?: ReadonlySet<string>;
  onDeleteSession?: import("../intents/sessionDeletionIntent").DeleteSessionAction;
  environmentLabel?: string;
  onArchiveNativeSession: (session: AgentListedSession) => void;
  onForkNativeSession?: (session: AgentListedSession) => void;
  onForkTask?: (taskId: string) => void;
  onAddProject?: () => void;
  /** Asks App Server to read Agent sessions again; the view reports what changed. */
  onRefreshNavigation: () => void;
  /** The history rows each Project shows unless `historyLimitOverrides` names it. */
  historyLimit: number;
  /** Per-Project history limits the user changed: "Show more" raises one, collapsing sets 0. */
  historyLimitOverrides: Readonly<Record<string, number>>;
  /** Sets a Project's history limit, or clears it (back to `historyLimit`) with `undefined`. */
  onSetProjectHistoryLimit: (projectId: string, historyLimit: number | undefined) => void;
  onBrowseProjectFiles?: (projectId: string) => void;
  onManageWorktrees?: (projectId: string) => void;
  onRemoveProject?: (project: {
    projectId: string;
    label: string;
    nativeSessionCount: number;
    taskCount: number;
  }) => void;
  onRenameProject?: (projectId: string, label: string) => Promise<void>;
  onNewTask: (projectId?: string) => void;
  onOpenNativeSession: (session: AgentListedSession) => void;
  onOpenWorkspaceFolder?: () => void;
  onOpenTask: (taskId: string) => void;
  onRecoverNativeSessions?: (kind: NonNullable<AppState["newTask"]["nativeSessions"]["recoveryKind"]>) => void;
  onDisableRecoveredAgent?: (
    agentId: string,
    acceptedActiveWorkInterruption?: boolean,
  ) => Promise<AgentDisableOutcome>;
  onArchiveTask: (taskId: string) => void;
  onArchiveOlderTasks?: (cutoff: TaskArchiveOlderCutoff, preview: boolean) => Promise<TaskArchiveOlderResult>;
  onRestoreNativeSession: (session: AgentListedSession) => void;
  onSetNativeSessionPinned?: (session: AgentListedSession, pinned: boolean) => Promise<void>;
  onSetNativeSessionTitle?: (session: AgentListedSession, title: string) => Promise<void>;
  onRestoreTask: (taskId: string) => void;
  onSetTaskPinned?: (taskId: string, pinned: boolean) => Promise<void>;
  onSetTaskTitle?: (
    taskId: string,
    title: { kind: "user"; value: string } | { kind: "automatic" },
  ) => Promise<void>;
  onSearchChange: (query: string) => void;
  /** Runs App Server's one-shot search; while the field has text its results replace the list. */
  onSearchNavigation: RunNavigationSearch;
  onSettings: () => void;
  onToggleArchived: () => void;
  searchQuery: string;
  settingsStatus?: string;
  settingsActive?: boolean;
  /** The shell's computers; the switcher appears once one other than this device exists. */
  computers?: ComputersSnapshot;
  computerLink?: ComputerLink;
  /** Increments when a shell command asks to open the computer list. */
  computerSwitcherRequest?: number;
  /** Present when the shell can switch computers. */
  computerSwitch?: ComputerSwitch;
  /** The switcher's last item (ADR-0081). */
  onUseAnotherComputer?: () => void;
  showArchived: boolean;
  taskListError?: string;
  hiddenFromAccessibility?: boolean;
  modal?: boolean;
  projects?: ProjectOption[];
  loadingTasks?: boolean;
  /** The label of an Agent whose managed integration is installing, if any. */
  installingAgentLabel?: string;
};

export const Sidebar = memo(function Sidebar({
  activeTaskId,
  agentIcons,
  nativeSessions,
  nativeSessionMutations = {},
  nativeSessionAgentId,
  nativeSessionAgentName,
  forkableAgentIds = new Set(),
  deletableAgentIds = new Set(),
  onDeleteSession,
  environmentLabel,
  onArchiveNativeSession,
  onForkNativeSession,
  onForkTask,
  onAddProject,
  onRefreshNavigation,
  historyLimit,
  historyLimitOverrides,
  onSetProjectHistoryLimit,
  onBrowseProjectFiles,
  onManageWorktrees,
  onRemoveProject,
  onRenameProject,
  onNewTask,
  onOpenNativeSession,
  onOpenWorkspaceFolder,
  onOpenTask,
  onRecoverNativeSessions,
  onDisableRecoveredAgent,
  onArchiveTask,
  onArchiveOlderTasks,
  onRestoreNativeSession,
  onSetNativeSessionPinned,
  onSetNativeSessionTitle,
  onRestoreTask,
  onSetTaskPinned,
  onSetTaskTitle,
  onSearchChange,
  onSearchNavigation,
  onSettings,
  onToggleArchived,
  searchQuery,
  settingsStatus,
  computers,
  computerLink,
  computerSwitcherRequest,
  computerSwitch,
  onUseAnotherComputer,
  settingsActive = false,
  showArchived,
  taskListError,
  hiddenFromAccessibility = false,
  modal = false,
  projects = [],
  loadingTasks = false,
  installingAgentLabel,
}: SidebarProps) {
  const taskListRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const taskListOverflow = useScrollOverflow(taskListRef, showArchived);
  // The recovery banner disables a recovered Agent here, so the confirmation it needs has to
  // appear here too; a Settings-only error would be invisible where the user clicked.
  const { cancelDisable, confirmDisable, pendingDisable, requestDisable } = useAgentDisableConfirmation();
  const [collapsedProjectKeys, setCollapsedProjectKeys] = useState<Set<string>>(() => new Set());
  // App Server owns the groups, their order, the window and the counts; typing in the field
  // leaves this list for the search results below rather than filtering it.
  const groups = nativeSessions.groups;
  const hasSearchInput = searchQuery.length > 0;
  const hasSearchQuery = searchQuery.trim().length > 0;
  const projectPageSize = taskNavigationPageSize(groups.length);
  const projectCollapsed = (projectKey: string) => collapsedProjectKeys.has(projectKey);
  const showEmptyState = !taskListError && !installingAgentLabel && groups.length === 0;
  const showWorkspaceSetup = !showArchived && onOpenWorkspaceFolder !== undefined;
  const showSessionRefresh = !showArchived && !showWorkspaceSetup;
  const noProjects = projects.length === 0;
  const refreshBusy = nativeSessions.loading || nativeSessions.adoptingSessionId !== undefined;
  // Touch refreshes by pulling the list down; the heading button stays for pointers and is hidden on phones.
  const pullToRefresh = usePullToRefresh(taskListRef, () => onRefreshNavigation(), {
    busy: refreshBusy,
    enabled: showSessionRefresh,
  });

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
          <span>
            <strong>Archive</strong>
            <small>{hasSearchQuery ? "Search results" : "Tasks and Native Sessions"}</small>
          </span>
        </div>
      ) : null}
      <div className={`sidebar-actions ${showArchived ? "archive-actions" : ""}`}>
        {!showArchived ? <button disabled={noProjects} type="button" onClick={() => onNewTask()}>
          <Plus size={15} />
          New task
        </button> : null}
        <div className={`sidebar-search ${hasSearchQuery ? "search-active" : ""}`}>
          <Search size={15} />
          <input
            aria-label={showArchived ? "Search archive" : "Search tasks"}
            disabled={noProjects}
            onChange={(event) => onSearchChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape" && hasSearchInput) {
                event.preventDefault();
                onSearchChange("");
              }
            }}
            placeholder={showArchived ? "Search archive" : "Search"}
            ref={searchInputRef}
            value={searchQuery}
          />
          {hasSearchInput ? (
            <button
              aria-label={showArchived ? "Clear archive search" : "Clear task search"}
              className="sidebar-search-clear"
              onClick={() => {
                onSearchChange("");
                searchInputRef.current?.focus();
              }}
              title="Clear search"
              type="button"
            >
              <X size={13} />
            </button>
          ) : null}
        </div>
      </div>
      {!showArchived ? <div className="task-section-head">
        <span aria-live="polite" className="task-section-title">
          {hasSearchQuery ? "Search results" : "Tasks"}
        </span>
        {showSessionRefresh ? (
          <span className="task-section-tools">
            <button
              aria-label="Refresh tasks"
              className="task-section-refresh"
              disabled={refreshBusy}
              onClick={() => onRefreshNavigation()}
              title={nativeSessions.loading ? "Refreshing tasks" : "Refresh tasks"}
              type="button"
            >
              <RefreshCcw size={13} />
            </button>
          </span>
        ) : null}
        <button className="archive-navigation" onClick={onToggleArchived} type="button"><Archive size={13} />Archive</button>
      </div> : null}
      {hasSearchQuery ? (
        <SidebarSearchResults
          activeTaskId={activeTaskId}
          agentIcons={agentIcons}
          onOpenNativeSession={onOpenNativeSession}
          onOpenTask={onOpenTask}
          query={searchQuery}
          search={onSearchNavigation}
        />
      ) : <SidebarTaskPreviewProvider><div
        className="task-list-shell"
        data-more-above={String(taskListOverflow.moreAbove)}
        data-more-below={String(taskListOverflow.moreBelow)}
      >{showSessionRefresh && (pullToRefresh.pull > 0 || pullToRefresh.refreshing) ? (
        <div
          aria-hidden="true"
          className="task-pull-refresh"
          data-armed={pullToRefresh.armed || undefined}
          data-loading={(pullToRefresh.refreshing && pullToRefresh.pull === 0) || undefined}
          style={{ "--task-pull-distance": `${pullToRefresh.pull}px` } as import("react").CSSProperties}
        >
          <RefreshCcw size={15} />
        </div>
      ) : null}<div
        className="task-list"
        role="list"
        aria-label={showArchived ? "Archived tasks" : "Tasks"}
        onScroll={taskListOverflow.onScroll}
        {...pullToRefresh.handlers}
        ref={taskListRef}
      >
        {!showWorkspaceSetup && taskListError ? <p className="empty-list">{taskListError}</p> : null}
        {showWorkspaceSetup
          ? <WorkspaceSetupPrompt compact onOpenFolder={onOpenWorkspaceFolder} />
          : null}
        {!showWorkspaceSetup && showEmptyState
          ? <p className="empty-list">{emptyListMessage({ loading: loadingTasks || nativeSessions.loading, showArchived })}</p>
          : null}
        {!showWorkspaceSetup && !showArchived && nativeSessions.error ? (
          <div className="native-session-recovery" role="status">
            <span>{nativeSessions.error}</span>
            {nativeSessions.recoveryKind && onRecoverNativeSessions ? (
              <div className="native-session-recovery-actions">
                <button type="button" onClick={() => onRecoverNativeSessions(nativeSessions.recoveryKind!)}>
                  {nativeSessions.recoveryKind === "authRequired"
                    ? "Sign in"
                    : nativeSessions.recoveryKind === "launchFailed" ? "Try again" : `Set up ${nativeSessions.recoveryAgentLabel ?? "the Agent"}`}
                </button>
                {nativeSessions.recoveryKind === "authRequired" && nativeSessions.recoveryAgentId && onDisableRecoveredAgent ? (
                  <button
                    type="button"
                    onClick={() => requestDisable(
                      nativeSessions.recoveryAgentId!,
                      (acceptedActiveWorkInterruption) =>
                        onDisableRecoveredAgent(nativeSessions.recoveryAgentId!, acceptedActiveWorkInterruption),
                    )}
                  >
                    {`Disable ${nativeSessions.recoveryAgentLabel ?? "this Agent"}`}
                  </button>
                ) : null}
              </div>
            ) : null}
          </div>
        ) : null}
        {!showWorkspaceSetup && !showArchived && installingAgentLabel ? (
          <div aria-busy={true} className="native-session-activity" role="status">
            <LoaderCircle aria-hidden="true" className="spin" size={14} />
            <span>
              {agentIntegrationInstallingLabel(installingAgentLabel)}
              <small>This can take a minute.</small>
            </span>
          </div>
        ) : null}
        {!showWorkspaceSetup && groups.map((group) => {
          const projectId = group.projectId;
          const project = projects.find((candidate) => candidate.projectId === projectId);
          return (
            <SidebarProjectTaskGroup
              activeTaskId={activeTaskId}
              agentIcons={agentIcons}
              collapsed={projectCollapsed(projectId)}
              group={group}
              key={projectId}
              workspaceRoot={project?.workspaceRoot}
              historyLimit={historyLimitOverrides[projectId] ?? historyLimit}
              pageSize={projectPageSize}
              nativeSessionAgentId={nativeSessionAgentId}
              nativeSessionAgentName={nativeSessionAgentName}
              nativeSessionMutations={nativeSessionMutations}
              nativeSessionsAdoptingSessionId={nativeSessions.adoptingSessionId}
              canManageWorktrees={Boolean(project?.worktreeRepositoryId)}
              forkableAgentIds={forkableAgentIds}
              deletableAgentIds={deletableAgentIds}
              onDeleteSession={onDeleteSession}
              environmentLabel={environmentLabel}
              onArchiveNativeSession={onArchiveNativeSession}
              onArchiveOlderNativeSessions={onArchiveOlderTasks}
              onArchiveTask={onArchiveTask}
              onArchiveOlderTasks={onArchiveOlderTasks}
              onForkNativeSession={onForkNativeSession}
              onForkTask={onForkTask}
              onLoadMore={(limit) => onSetProjectHistoryLimit(projectId, limit)}
              onBrowseFiles={onBrowseProjectFiles ? () => onBrowseProjectFiles(projectId) : undefined}
              onManageWorktrees={onManageWorktrees ? () => onManageWorktrees(projectId) : undefined}
              onNewTask={() => onNewTask(projectId)}
              onRemoveProject={onRemoveProject ? () => onRemoveProject({
                projectId,
                label: group.label,
                nativeSessionCount: group.rows.filter((row) => row.kind === "session").length,
                taskCount: group.total,
              }) : undefined}
              onRenameProject={onRenameProject ? (label) => onRenameProject(projectId, label) : undefined}
              onOpenNativeSession={onOpenNativeSession}
              onOpenTask={onOpenTask}
              onRestoreNativeSession={onRestoreNativeSession}
              onSetNativeSessionPinned={onSetNativeSessionPinned}
              onSetNativeSessionTitle={onSetNativeSessionTitle}
              onRestoreTask={onRestoreTask}
              onSetTaskPinned={onSetTaskPinned}
              onSetTaskTitle={onSetTaskTitle}
              onToggleCollapse={() => {
                const collapsing = !collapsedProjectKeys.has(projectId);
                setCollapsedProjectKeys((current) => {
                  const next = new Set(current);
                  if (collapsing) next.add(projectId);
                  else next.delete(projectId);
                  return next;
                });
                // Collapsing hides the history rows here at once and asks App Server to stop
                // sending them; expanding goes back to the default window.
                onSetProjectHistoryLimit(projectId, collapsing ? 0 : undefined);
              }}
              showArchived={showArchived}
            />
          );
        })}
      </div></div></SidebarTaskPreviewProvider>}
      <div className="sidebar-footer">
        {!showArchived && onAddProject ? (
          <button className="add-project-button" onClick={onAddProject} type="button">
            <FolderPlus size={15} />
            Add project
          </button>
        ) : null}
        <div className="sidebar-settings-row">
          <button
            aria-current={settingsActive ? "page" : undefined}
            className={`settings-button ${settingsActive ? "selected" : ""}`}
            type="button"
            onClick={onSettings}
          >
            <Settings size={15} />
            <span className="settings-button-copy">
              <span>Settings</span>
              {settingsStatus ? <small>{settingsStatus}</small> : null}
            </span>
          </button>
          {computerSwitch && onUseAnotherComputer ? (
            <ComputerSwitcher
              link={computerLink ?? "connecting"}
              onUseAnotherComputer={onUseAnotherComputer}
              openRequest={computerSwitcherRequest}
              snapshot={computers}
              switcher={computerSwitch}
            />
          ) : null}
        </div>
      </div>
      {pendingDisable ? (
        <AgentDisableDialog
          agentLabel={nativeSessions.recoveryAgentLabel ?? "this Agent"}
          onCancel={cancelDisable}
          onConfirm={confirmDisable}
          runningTaskCount={pendingDisable.runningTaskCount}
        />
      ) : null}
    </aside>
  );
}, sameSidebarDataProps);

function sameSidebarDataProps(prev: SidebarProps, next: SidebarProps) {
  return prev.activeTaskId === next.activeTaskId &&
    prev.nativeSessions === next.nativeSessions &&
    prev.nativeSessionMutations === next.nativeSessionMutations &&
    prev.nativeSessionAgentId === next.nativeSessionAgentId &&
    prev.nativeSessionAgentName === next.nativeSessionAgentName &&
    prev.forkableAgentIds === next.forkableAgentIds &&
    prev.deletableAgentIds === next.deletableAgentIds &&
    prev.onDeleteSession === next.onDeleteSession &&
    prev.onOpenWorkspaceFolder === next.onOpenWorkspaceFolder &&
    prev.searchQuery === next.searchQuery &&
    prev.onSearchNavigation === next.onSearchNavigation &&
    prev.settingsStatus === next.settingsStatus &&
    prev.computers === next.computers &&
    prev.computerLink === next.computerLink &&
    prev.computerSwitcherRequest === next.computerSwitcherRequest &&
    prev.computerSwitch === next.computerSwitch &&
    prev.settingsActive === next.settingsActive &&
    prev.showArchived === next.showArchived &&
    prev.taskListError === next.taskListError &&
    prev.historyLimit === next.historyLimit &&
    prev.historyLimitOverrides === next.historyLimitOverrides &&
    prev.hiddenFromAccessibility === next.hiddenFromAccessibility &&
    prev.modal === next.modal &&
    prev.projects === next.projects &&
    prev.loadingTasks === next.loadingTasks &&
    prev.installingAgentLabel === next.installingAgentLabel;
}

function emptyListMessage({ loading, showArchived }: { loading: boolean; showArchived: boolean }) {
  if (showArchived) return "Archive is empty. Archived Tasks and Native Sessions will appear here.";
  return loading ? "Loading tasks." : "No tasks yet.";
}
