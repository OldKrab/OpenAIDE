import { Menu, X } from "lucide-react";
import { useEffect, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { AppPrimaryTaskSurface, createAgentRecoveryActions, primaryTaskSurfaceModel } from "./AppPrimaryTaskSurface";
import { DEFAULT_MAX_TASKS_PER_PROJECT, Sidebar } from "./Sidebar";
import { SettingsView } from "./settings/SettingsView";
import { taskStatusLabel } from "./TaskHeader";
import type { AppController } from "./appController";
import { useMobileNavigation } from "./useMobileNavigation";
import { useInputModality } from "./useInputModality";
import { useWebTaskNotifications } from "./useWebTaskNotifications";
import { TaskWorkspacePicker } from "./TaskWorkspacePicker";
import { updateTaskSurfaceTitle } from "../services/hostBridge";

export function AppSurfaces({ controller }: { controller: AppController }) {
  useInputModality();
  const taskNotifications = useWebTaskNotifications(controller);
  const { activeNavigationTaskId, activeTask, backendReady, bootstrap, callbacks, preferences, view, visibleTasks } = controller;
  const { appServerError, navigation, settings } = view;
  // The App Server Project catalog is global; current-Project shells expose only
  // the ordered Project identities represented by this App Shell workspace.
  const currentNavigationProjectIds = bootstrap.surface !== "invalid"
    && bootstrap.shell.navigationMode === "currentProject"
    ? bootstrap.projectIds ?? (bootstrap.projectId ? [bootstrap.projectId] : undefined)
    : undefined;
  const navigationProjects = currentNavigationProjectIds === undefined
    ? navigation.projects
    : currentNavigationProjectIds.flatMap((projectId) => {
        const project = navigation.projects.find((candidate) => candidate.projectId === projectId);
        return project ? [project] : [];
      });
  const [mobileLayoutActive, setMobileLayoutActive] = useState(() => isMobileWebViewport());
  const [newTaskFocusRequestKey, setNewTaskFocusRequestKey] = useState(0);
  const [managedProjectSurface, setManagedProjectSurface] = useState<{
    projectId: string;
    surfaceKey: string;
  }>();
  const mobileNavigationButtonRef = useRef<HTMLButtonElement | null>(null);
  const webMainSurfaceRef = useRef<HTMLElement | null>(null);
  const surfaceKey = appSurfaceKey(bootstrap);
  const surfaceKeyRef = useRef(surfaceKey);
  surfaceKeyRef.current = surfaceKey;
  const isWebShell = bootstrap.surface !== "invalid" && bootstrap.shell.kind === "web";
  const isWebWorkbench = isWebShell && (
    bootstrap.surface === "task"
    || bootstrap.surface === "nativeSession"
    || bootstrap.surface === "settings"
  );
  const sidebarActiveTaskId = bootstrap.surface === "settings"
    ? undefined
    : bootstrap.surface === "task"
      ? bootstrap.taskId
      : activeNavigationTaskId;
  const mobileNavigation = useMobileNavigation(isWebWorkbench && mobileLayoutActive);
  const mobileNavigationOpen = mobileNavigation.open;
  const taskSurfaceModel = primaryTaskSurfaceModel(controller);
  const taskRecoveryActions = createAgentRecoveryActions(controller);
  const settingsRecoveryActions = {
    ...taskRecoveryActions,
    onRetry: async (agentId: string) => {
      const ready = await taskRecoveryActions.onRetry(agentId);
      callbacks.settings.refreshSettings();
      return ready;
    },
  };
  const authenticateAndReturn = async (agentId: string, methodId: string, values?: Record<string, string>) => {
    const authenticated = await callbacks.settings.authenticateAgent(agentId, methodId, values);
    if (authenticated && bootstrap.surface !== "invalid" && bootstrap.returnToNewTask) {
      callbacks.navigation.openNewTask(bootstrap.projectId);
    }
    return authenticated;
  };
  const { openingNativeSession, renderableTaskSnapshot } = taskSurfaceModel;
  useEffect(() => {
    if (
      bootstrap.surface !== "task"
      || bootstrap.shell.kind !== "vscodeExtension"
      || !renderableTaskSnapshot
      || renderableTaskSnapshot.task.task_id !== bootstrap.taskId
    ) return;
    updateTaskSurfaceTitle(
      renderableTaskSnapshot.task.task_id,
      renderableTaskSnapshot.task.title,
    );
  }, [
    bootstrap.surface,
    bootstrap.surface === "task" ? bootstrap.taskId : undefined,
    bootstrap.surface === "invalid" ? undefined : bootstrap.shell.kind,
    renderableTaskSnapshot?.task.task_id,
    renderableTaskSnapshot?.task.title,
  ]);
  const managedProjectId = managedProjectSurface?.surfaceKey === surfaceKey
    ? managedProjectSurface.projectId
    : undefined;
  useEffect(() => {
    if (managedProjectSurface && managedProjectSurface.surfaceKey !== surfaceKey) {
      setManagedProjectSurface(undefined);
    }
  }, [managedProjectSurface, surfaceKey]);
  const manageWorktrees = (projectId: string) => {
    // Sidebar intentionally ignores callback identity while memoizing, so read the current owner lazily.
    setManagedProjectSurface({ projectId, surfaceKey: surfaceKeyRef.current });
  };
  const managedProject = navigationProjects.find((project) => project.projectId === managedProjectId);
  const managedRepository = managedProject?.worktreeRepositoryId
    ? view.primaryTask.newTask.worktreeRepositories[managedProject.worktreeRepositoryId]
    : undefined;
  const managementSurface = managedProject ? (
    <TaskWorkspacePicker
      initialMode="manage"
      intents={controller.intents.newTask}
      managementOnly
      onClose={() => setManagedProjectSurface(undefined)}
      onUseForNewTask={() => callbacks.navigation.openNewTask(managedProject.projectId)}
      project={managedProject}
      repository={managedRepository}
      selectedWorktreeId={view.primaryTask.newTask.newTask.selection.worktreeId}
      tasks={view.primaryTask.newTask.tasks}
    />
  ) : null;
  const closeMobileNavigation = ({ restoreFocus = true }: { restoreFocus?: boolean } = {}) => {
    mobileNavigation.setOpen(false);
    if (restoreFocus && typeof window !== "undefined") {
      window.requestAnimationFrame?.(() => mobileNavigationButtonRef.current?.focus());
    }
  };
  const requestNewTaskFocus = () => setNewTaskFocusRequestKey((key) => key + 1);
  useEffect(() => {
    if (!mobileNavigationOpen) return;
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") closeMobileNavigation();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [mobileNavigationOpen]);
  useEffect(() => {
    if (!isWebWorkbench || typeof window === "undefined") return;
    const mediaQuery = typeof window.matchMedia === "function"
      ? window.matchMedia("(max-width: 760px)")
      : undefined;
    const syncMobileLayout = () => {
      setMobileLayoutActive(mediaQuery ? mediaQuery.matches : isMobileWebViewport());
    };
    syncMobileLayout();
    mediaQuery?.addEventListener?.("change", syncMobileLayout);
    return () => mediaQuery?.removeEventListener?.("change", syncMobileLayout);
  }, [isWebWorkbench]);
  useEffect(() => {
    const mainSurface = webMainSurfaceRef.current;
    if (!mainSurface) return;
    if (mobileNavigationOpen) {
      mainSurface.setAttribute("inert", "");
      mainSurface.setAttribute("aria-hidden", "true");
      mobileNavigationButtonRef.current?.focus();
      return () => {
        mainSurface.removeAttribute("aria-hidden");
        mainSurface.removeAttribute("inert");
      };
    }
    mainSurface.removeAttribute("aria-hidden");
    mainSurface.removeAttribute("inert");
    return undefined;
  }, [mobileNavigationOpen]);

  const trapMobileNavigationFocus = (event: ReactKeyboardEvent<HTMLElement>) => {
    if (!mobileNavigationOpen || event.key !== "Tab") return;
    const focusable = mobileNavigationFocusableElements(event.currentTarget);
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;
    if (event.shiftKey && active === first) {
      event.preventDefault();
      last.focus();
      return;
    }
    if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  };
  if (bootstrap.surface === "invalid") {
    return (
      <main className="app-shell editor-shell">
        <section className="task-surface task-loading" aria-label="Invalid OpenAIDE surface">
          <p>OpenAIDE could not open this view.</p>
        </section>
      </main>
    );
  }

  if (bootstrap.surface === "navigation") {
    return (
      <main className="app-shell navigation-shell">
        <Sidebar
          activeTaskId={activeNavigationTaskId}
          groupByProject={true}
          maxTasksPerProject={DEFAULT_MAX_TASKS_PER_PROJECT}
          nativeSessions={navigation.nativeSessions}
          nativeSessionMutations={navigation.nativeSessionMutations}
          nativeSessionAgentId={navigation.newTaskSelection.agentId}
          nativeSessionAgentName={navigation.newTaskSelection.agentLabel}
          nativeSessionProjectId={navigation.newTaskSelection.projectId}
          onArchiveTask={callbacks.navigation.archiveTask}
          onArchiveNativeSession={callbacks.navigation.archiveNativeSession}
          onLoadNativeSessions={callbacks.navigation.loadNativeSessions}
          onManageWorktrees={manageWorktrees}
          onNewTask={callbacks.navigation.openNewTask}
          onOpenNativeSession={callbacks.navigation.openNativeSession}
          onOpenWorkspaceFolder={controller.workspaceSetup?.openFolder}
          onOpenTask={callbacks.navigation.openTask}
          onRecoverNativeSessions={(kind) => kind === "launchFailed"
            ? callbacks.navigation.loadNativeSessions()
            : callbacks.navigation.openSettings()}
          onRestoreTask={callbacks.navigation.restoreTask}
          onSetTaskTitle={callbacks.navigation.setTaskTitle}
          onRestoreNativeSession={callbacks.navigation.restoreNativeSession}
          onSearchChange={callbacks.navigation.changeSearch}
          onSettings={callbacks.navigation.openSettings}
          onToggleArchived={callbacks.navigation.toggleArchived}
          projects={navigationProjects}
          searchQuery={navigation.searchQuery}
          showArchived={navigation.showArchived}
          taskListError={navigation.taskListError}
          tasks={visibleTasks}
        />
        {managementSurface}
      </main>
    );
  }

  if (appServerError && !isWebShell) {
    return (
      <main className="app-shell editor-shell">
        <AppServerErrorView message={appServerError} />
      </main>
    );
  }

  if (bootstrap.surface === "settings" && !isWebShell) {
    return (
      <main className="app-shell editor-shell">
        <SettingsView
          onAuthenticate={authenticateAndReturn}
          onCreateCustomAgent={callbacks.settings.createCustomAgent}
          onDeleteCustomAgent={callbacks.settings.deleteCustomAgent}
          onRefresh={callbacks.settings.refreshSettings}
          onReplaceCustomAgent={callbacks.settings.replaceCustomAgent}
          onSelectTab={callbacks.settings.selectSettingsTab}
          onSetAcpTrace={callbacks.settings.setAcpTrace}
          onSetAgentEnabled={callbacks.settings.setAgentEnabled}
          onSetComposerSubmitShortcut={callbacks.settings.setComposerSubmitShortcut}
          onUpdateCustomAgentMetadata={callbacks.settings.updateCustomAgentMetadata}
          onUnlockDeveloperSettings={callbacks.settings.unlockDeveloperSettings}
          preferences={preferences}
          preferredAgentId={bootstrap.settingsAgentId}
          recoveryActions={settingsRecoveryActions}
          state={settings}
        />
      </main>
    );
  }

  if (isWebWorkbench) {
    const routedActiveTask = bootstrap.taskId ? activeTask : undefined;
    const mobileTitle = bootstrap.surface === "settings"
      ? "Settings"
      : renderableTaskSnapshot?.task.title ?? routedActiveTask?.title ?? (openingNativeSession ? "Opening session" : bootstrap.taskId ? "Opening task" : "New task");
    const mobileProject = activeTask?.project_label ?? navigation.projects[0]?.label ?? "OpenAIDE";
    const mobileTaskStatus = renderableTaskSnapshot?.task.status ?? routedActiveTask?.status;
    const mobileSubtitle = bootstrap.surface === "settings"
      ? "Agent and app configuration"
      : [mobileTaskStatus ? taskStatusLabel(mobileTaskStatus) : undefined, mobileProject].filter(Boolean).join(" · ");
    const closeAfter = <T extends unknown[]>(callback: (...args: T) => void) => (...args: T) => {
      closeMobileNavigation();
      callback(...args);
    };
    const openNewTaskFromNavigation = (projectId?: string) => {
      closeMobileNavigation({ restoreFocus: false });
      requestNewTaskFocus();
      callbacks.navigation.openNewTask(projectId);
    };
    return (
      <main
        className={[
          "app-shell web-workbench-shell",
          mobileNavigationOpen ? "mobile-navigation-open" : undefined,
          mobileNavigation.dragging ? "mobile-navigation-dragging" : undefined,
        ].filter(Boolean).join(" ")}
        onKeyDown={trapMobileNavigationFocus}
        onPointerCancel={mobileNavigation.cancelSwipe}
        onPointerDownCapture={mobileNavigation.beginSwipe}
        onPointerMoveCapture={mobileNavigation.trackSwipe}
        onPointerUp={mobileNavigation.endSwipe}
        style={mobileNavigation.dragProgress === undefined
          ? undefined
          : { "--mobile-navigation-progress": mobileNavigation.dragProgress } as CSSProperties}
      >
        <header className="mobile-workbench-bar">
          <button
            aria-expanded={mobileNavigationOpen}
            aria-label={mobileNavigationOpen ? "Close task navigation" : "Open task navigation"}
            onClick={() => {
              if (mobileNavigationOpen) {
                closeMobileNavigation();
                return;
              }
              mobileNavigation.setOpen(true);
            }}
            ref={mobileNavigationButtonRef}
            type="button"
          >
            {mobileNavigationOpen ? <X size={17} /> : <Menu size={17} />}
          </button>
          <span>
            <strong>{mobileTitle}</strong>
            <small>{mobileSubtitle}</small>
          </span>
        </header>
        <div
          aria-hidden="true"
          className="mobile-navigation-backdrop"
          onClick={() => closeMobileNavigation()}
        />
        <section
          aria-hidden={mobileNavigation.active ? true : undefined}
          className="web-main-surface"
          inert={mobileNavigation.active ? true : undefined}
          ref={webMainSurfaceRef}
        >
          {appServerError ? (
            <AppServerErrorView message={appServerError} />
          ) : bootstrap.surface === "settings" ? (
            <SettingsView
              desktopNotifications={taskNotifications?.settings}
              onAuthenticate={authenticateAndReturn}
              onCreateCustomAgent={callbacks.settings.createCustomAgent}
              onDeleteCustomAgent={callbacks.settings.deleteCustomAgent}
              onRefresh={callbacks.settings.refreshSettings}
              onReplaceCustomAgent={callbacks.settings.replaceCustomAgent}
              onSelectTab={callbacks.settings.selectSettingsTab}
              onSetDesktopNotifications={taskNotifications?.setEnabled}
              onSetAcpTrace={callbacks.settings.setAcpTrace}
              onSetAgentEnabled={callbacks.settings.setAgentEnabled}
              onSetComposerSubmitShortcut={callbacks.settings.setComposerSubmitShortcut}
              onUpdateCustomAgentMetadata={callbacks.settings.updateCustomAgentMetadata}
              onUnlockDeveloperSettings={callbacks.settings.unlockDeveloperSettings}
              preferences={preferences}
              preferredAgentId={bootstrap.settingsAgentId}
              recoveryActions={settingsRecoveryActions}
              state={settings}
            />
          ) : (
            <AppPrimaryTaskSurface
              controller={controller}
              focusRequestKey={newTaskFocusRequestKey}
              model={taskSurfaceModel}
              workspaceRecovery={{ manageWorktrees, openProjectSettings: callbacks.navigation.openSettings, reconnectProject: callbacks.navigation.openNewTask }}
            />
          )}
        </section>
        <Sidebar
          activeTaskId={sidebarActiveTaskId}
          groupByProject={true}
          hiddenFromAccessibility={mobileLayoutActive && !mobileNavigation.active}
          maxTasksPerProject={DEFAULT_MAX_TASKS_PER_PROJECT}
          modal={mobileLayoutActive && mobileNavigation.active}
          loadingTasks={!backendReady}
          nativeSessions={navigation.nativeSessions}
          nativeSessionMutations={navigation.nativeSessionMutations}
          nativeSessionAgentId={navigation.newTaskSelection.agentId}
          nativeSessionAgentName={navigation.newTaskSelection.agentLabel}
          nativeSessionProjectId={navigation.newTaskSelection.projectId}
          onArchiveNativeSession={callbacks.navigation.archiveNativeSession}
          onArchiveTask={callbacks.navigation.archiveTask}
          onLoadNativeSessions={callbacks.navigation.loadNativeSessions}
          onManageWorktrees={(projectId) => { closeMobileNavigation({ restoreFocus: false }); manageWorktrees(projectId); }}
          onNewTask={openNewTaskFromNavigation}
          onOpenNativeSession={closeAfter(callbacks.navigation.openNativeSession)}
          onOpenTask={closeAfter(callbacks.navigation.openTask)}
          onRecoverNativeSessions={(kind) => kind === "launchFailed"
            ? callbacks.navigation.loadNativeSessions()
            : callbacks.navigation.openSettings()}
          onRestoreTask={callbacks.navigation.restoreTask}
          onSetTaskTitle={callbacks.navigation.setTaskTitle}
          onRestoreNativeSession={callbacks.navigation.restoreNativeSession}
          onSearchChange={callbacks.navigation.changeSearch}
          onSettings={closeAfter(callbacks.navigation.openSettings)}
          onToggleArchived={callbacks.navigation.toggleArchived}
          projects={navigation.projects}
          searchQuery={navigation.searchQuery}
          settingsActive={bootstrap.surface === "settings"}
          showArchived={navigation.showArchived}
          taskListError={navigation.taskListError}
          tasks={visibleTasks}
        />
        {managementSurface}
      </main>
    );
  }

  return (
    <main className="app-shell editor-shell">
      <AppPrimaryTaskSurface
        controller={controller}
        focusRequestKey={newTaskFocusRequestKey}
        model={taskSurfaceModel}
        workspaceRecovery={{ manageWorktrees, openProjectSettings: callbacks.navigation.openSettings, reconnectProject: callbacks.navigation.openNewTask }}
      />
      {managementSurface}
    </main>
  );
}

function isMobileWebViewport() {
  if (typeof window === "undefined") return false;
  if (typeof window.matchMedia === "function") {
    return window.matchMedia("(max-width: 760px)").matches;
  }
  return window.innerWidth <= 760;
}

function appSurfaceKey(bootstrap: AppController["bootstrap"]): string {
  if (bootstrap.surface === "invalid") return "invalid";
  if (bootstrap.surface === "task") {
    return JSON.stringify(bootstrap.taskId
      ? ["task", bootstrap.taskId]
      : ["new-task", bootstrap.projectId ?? null]);
  }
  if (bootstrap.surface === "nativeSession") {
    return JSON.stringify(["native-session", bootstrap.agentId ?? null, bootstrap.nativeSessionId ?? null]);
  }
  if (bootstrap.surface === "navigation") {
    return JSON.stringify([
      "navigation",
      bootstrap.archived ? "archived" : "active",
      bootstrap.projectIds ?? (bootstrap.projectId ? [bootstrap.projectId] : []),
    ]);
  }
  return JSON.stringify(["settings", bootstrap.settingsTab ?? null, bootstrap.settingsAgentId ?? null]);
}

function mobileNavigationFocusableElements(root: HTMLElement) {
  return Array.from(
    root.querySelectorAll<HTMLElement>(
      ".mobile-workbench-bar button, .sidebar button:not(:disabled), .sidebar input:not(:disabled)",
    ),
  ).filter((element) => element.offsetParent !== null);
}

function AppServerErrorView({ message }: { message: string }) {
  return (
    <section
      aria-label="App Server connection error"
      aria-live="polite"
      className="task-surface task-loading app-server-error"
    >
      <p>App Server connection unavailable.</p>
      <small>{message}</small>
    </section>
  );
}
