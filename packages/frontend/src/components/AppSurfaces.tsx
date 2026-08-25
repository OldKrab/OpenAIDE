import { ListTodo, Menu, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { AppSidebarFrame } from "./AppSidebarFrame";
import { AppPrimaryTaskSurface, createAgentRecoveryActions, primaryTaskSurfaceModel } from "./AppPrimaryTaskSurface";
import { DesktopTitleBar } from "./DesktopTitleBar";
import { Sidebar } from "./Sidebar";
import { SettingsView } from "./settings/SettingsView";
import { TaskPermissionPolicyControl } from "./TaskPermissionPolicyControl";
import { taskStatusLabel } from "./TaskHeader";
import type { AppController } from "./appController";
import { useMobileNavigation } from "./useMobileNavigation";
import { useInputModality } from "./useInputModality";
import { useWebTaskNotifications } from "./useWebTaskNotifications";
import { updateTaskSurfaceTitle } from "../services/hostBridge";
import { ProjectFolderDialog } from "./ProjectFolderDialog";
import { ProjectRemoveDialog } from "./ProjectRemoveDialog";
import { currentFrontendShell } from "../services/frontendShell";

export function AppSurfaces({ controller }: { controller: AppController }) {
  useInputModality();
  const taskNotifications = useWebTaskNotifications(controller);
  const {
    activeNavigationTaskId,
    activeTask,
    backendReady,
    bootstrap,
    callbacks,
    preferences,
    taskMutationReady,
    view,
    visibleTasks,
  } = controller;
  const { appServerError, navigation, settings } = view;
  const forkableAgentIds = useMemo(
    () => new Set((controller.agents ?? [])
      .filter((agent) => agent.capabilities?.forkNativeSessions)
      .map((agent) => agent.id)),
    [controller.agents],
  );
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
  const [planDrawerOpen, setPlanDrawerOpen] = useState(false);
  const [projectFolderDialogOpen, setProjectFolderDialogOpen] = useState(false);
  const [projectToRemove, setProjectToRemove] = useState<{
    projectId: string;
    label: string;
    nativeSessionCount: number;
    taskCount: number;
  }>();
  const mobileNavigationButtonRef = useRef<HTMLButtonElement | null>(null);
  const webMainSurfaceRef = useRef<HTMLElement | null>(null);
  const isWebShell = bootstrap.surface !== "invalid" && bootstrap.shell.kind === "web";
  const isProjectWorkbench = bootstrap.surface !== "invalid"
    && bootstrap.shell.navigationMode === "project" && (
    bootstrap.surface === "task"
    || bootstrap.surface === "nativeSession"
    || bootstrap.surface === "settings"
  );
  const isWebWorkbench = isWebShell && isProjectWorkbench;
  const sidebarActiveTaskId = bootstrap.surface === "settings"
    ? undefined
    : bootstrap.surface === "task"
      ? bootstrap.taskId
      : activeNavigationTaskId;
  const mobileNavigation = useMobileNavigation(isWebWorkbench && mobileLayoutActive);
  const mobileNavigationOpen = mobileNavigation.open;
  const taskSurfaceModel = primaryTaskSurfaceModel(controller);
  const mobilePermissionTask = isWebWorkbench ? taskSurfaceModel.renderableTaskSnapshot : undefined;
  const hasMobilePermissionPolicy = Boolean(mobilePermissionTask);
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
  const newTaskInWorktree = (
    project: (typeof navigationProjects)[number],
    worktree: (typeof view.primaryTask.newTask.worktreeRepositories)[string]["worktrees"][number],
  ) => {
    controller.intents.newTask.selectProject(project);
    controller.intents.newTask.selectWorktree({
      label: worktree.name,
      path: worktree.path,
      worktreeId: worktree.worktreeId,
    });
    callbacks.navigation.openNewTask(project.projectId);
  };
  const backFromSettings = isProjectWorkbench
    ? () => {
        if (activeNavigationTaskId) {
          callbacks.navigation.openTask(activeNavigationTaskId);
          return;
        }
        callbacks.navigation.openNewTask(
          bootstrap.surface === "settings" ? bootstrap.projectId : undefined,
        );
      }
    : undefined;
  const { openingNativeSession, renderableTaskArchived, renderableTaskSnapshot } = taskSurfaceModel;
  const routedTaskProjectId = bootstrap.surface === "task"
    ? (renderableTaskSnapshot && renderableTaskSnapshot.task.task_id === bootstrap.taskId
        ? renderableTaskSnapshot.task.project_id
        : activeTask && activeTask.task_id === bootstrap.taskId
          ? activeTask.project_id
          : visibleTasks.find((task) => task.task_id === bootstrap.taskId)?.project_id)
    : undefined;
  useEffect(() => setPlanDrawerOpen(false), [renderableTaskSnapshot?.task.task_id]);
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
  const manageWorktrees = (projectId: string) => {
    callbacks.navigation.openSettings(undefined, undefined, projectId, "worktrees");
  };
  const closeMobileNavigation = ({ restoreFocus = true }: { restoreFocus?: boolean } = {}) => {
    mobileNavigation.setOpen(false);
    if (restoreFocus && typeof window !== "undefined") {
      window.requestAnimationFrame?.(() => mobileNavigationButtonRef.current?.focus());
    }
  };
  const requestNewTaskFocus = () => setNewTaskFocusRequestKey((key) => key + 1);
  const frontendShell = currentFrontendShell();
  // Folder acquisition belongs to the App Shell: Web browses through the App
  // Server, Desktop opens the OS picker, and VS Code delegates to its host.
  const workspaceBrowser = isWebShell ? callbacks.newTask.workspaceBrowser : undefined;
  const workspaceCapability = bootstrap.surface !== "invalid" && bootstrap.shell.kind === "vscodeExtension"
    ? frontendShell?.workspace
    : undefined;
  const projectFolderPicker = bootstrap.surface !== "invalid" && bootstrap.shell.kind === "desktop"
    ? frontendShell?.projects
    : undefined;
  const desktopWindow = bootstrap.surface !== "invalid" && bootstrap.shell.kind === "desktop"
    ? frontendShell?.desktopWindow
    : undefined;
  const desktopRuntimeEnvironment = bootstrap.surface !== "invalid" && bootstrap.shell.kind === "desktop"
    ? frontendShell?.desktopRuntime?.snapshot().active
    : undefined;
  const desktopEnvironmentLabel = desktopRuntimeEnvironment?.kind === "wsl"
    ? `WSL · ${desktopRuntimeEnvironment.distro}`
    : desktopRuntimeEnvironment ? "Windows" : undefined;
  const finishAddingProject = async (folder: { path: string; label: string }) => {
    let project = await controller.intents.projects.add(folder.path);
    const label = folder.label.trim();
    if (label && label !== project.label) {
      project = await controller.intents.projects.rename(project.projectId, label);
    }
    setProjectFolderDialogOpen(false);
    controller.intents.newTask.selectProject({
      projectId: project.projectId,
      label: project.label,
      workspaceRoot: project.workspaceRoot,
      available: project.available,
      worktreeRepositoryId: project.worktreeRepositoryId ?? undefined,
      projectWorktreeId: project.projectWorktreeId ?? undefined,
      worktreeError: project.worktreeError ?? undefined,
    });
    callbacks.navigation.openNewTask(project.projectId);
  };
  const addProject = projectFolderPicker
    ? () => { void projectFolderPicker.pickFolder().then((folder) => folder && finishAddingProject(folder)); }
    : workspaceCapability
      ? () => workspaceCapability.openFolder()
      : workspaceBrowser
        ? () => setProjectFolderDialogOpen(true)
        : undefined;
  useEffect(() => frontendShell?.desktopCommands?.subscribe((command) => {
    if (command === "new-task") callbacks.navigation.openNewTask();
    else if (command === "settings") callbacks.navigation.openSettings();
    else if (command === "open-project") addProject?.();
  }), [addProject, callbacks.navigation, frontendShell]);
  const desktopTaskSnapshot = renderableTaskSnapshot?.task.has_messages
    ? renderableTaskSnapshot
    : undefined;
  const desktopTitleBar = desktopWindow
    ? <DesktopTitleBar window={desktopWindow} />
    : undefined;
  const desktopSettingsTitleBar = desktopWindow ? (
    <DesktopTitleBar window={desktopWindow} />
  ) : undefined;
  // Empty desktop chrome overlays New Task so platform controls remain available
  // without consuming a blank row from the working surface.
  const desktopTitleBarPlacement = desktopWindow && !desktopTaskSnapshot
    ? "overlay"
    : "row";
  const desktopSettingsTitleBarPlacement = desktopWindow?.platform === "macos"
    ? "overlay"
    : "row";
  const projectFolderDialog = projectFolderDialogOpen && workspaceBrowser ? (
    <ProjectFolderDialog
      browser={workspaceBrowser}
      onClose={() => setProjectFolderDialogOpen(false)}
      onSelect={finishAddingProject}
    />
  ) : null;
  const projectRemoveDialog = projectToRemove ? (
    <ProjectRemoveDialog
      onCancel={() => setProjectToRemove(undefined)}
      onConfirm={async () => {
        const removedProjectIndex = navigationProjects.findIndex(
          (project) => project.projectId === projectToRemove.projectId,
        );
        const adjacentProject = removedProjectIndex < 0
          ? undefined
          : navigationProjects[removedProjectIndex + 1] ?? navigationProjects[removedProjectIndex - 1];
        const removesRoutedTask = routedTaskProjectId === projectToRemove.projectId;
        await controller.intents.projects.remove(projectToRemove.projectId);
        setProjectToRemove(undefined);
        if (removesRoutedTask) {
          // The removed Task route is no longer valid. Select its adjacent Project before
          // routing so New Task never briefly reacquires the removed Project context.
          if (adjacentProject) controller.intents.newTask.selectProject(adjacentProject);
          callbacks.navigation.openNewTask(adjacentProject?.projectId);
        }
      }}
      project={projectToRemove}
    />
  ) : null;
  const prepareProjectRemoval = (project: {
    projectId: string;
    label: string;
    nativeSessionCount: number;
    taskCount: number;
  }) => {
    setProjectToRemove(project);
    void controller.intents.newTask.loadProjectTasks?.(project.projectId).then((tasks) => {
      setProjectToRemove((current) => current?.projectId === project.projectId
        ? { ...current, taskCount: tasks.length + current.nativeSessionCount }
        : current);
    }).catch(() => {
      // The confirmation remains usable with the navigation count; removal returns the authoritative count.
    });
  };
  const projectRemovalCounts = (projectId: string) => {
    const nativeSessionCount = navigation.nativeSessions.items
      .filter((session) => session.project_id === projectId).length;
    return {
      nativeSessionCount,
      taskCount: visibleTasks.filter((task) => task.project_id === projectId).length
        + nativeSessionCount,
    };
  };
  useEffect(() => {
    if (!mobileNavigationOpen) return;
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") closeMobileNavigation();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [mobileNavigationOpen]);
  useEffect(() => {
    if (!planDrawerOpen) return;
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") setPlanDrawerOpen(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [planDrawerOpen]);
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
          nativeSessions={navigation.nativeSessions}
          nativeSessionMutations={navigation.nativeSessionMutations}
          nativeSessionAgentId={navigation.newTaskSelection.agentId}
          nativeSessionAgentName={navigation.newTaskSelection.agentLabel}
          nativeSessionProjectId={navigation.newTaskSelection.projectId}
          forkableAgentIds={forkableAgentIds}
          environmentLabel={desktopEnvironmentLabel}
          onArchiveTask={callbacks.navigation.archiveTask}
          onArchiveOlderTasks={callbacks.navigation.archiveOlderTasks}
          onAddProject={addProject}
          onArchiveNativeSession={callbacks.navigation.archiveNativeSession}
          onForkNativeSession={callbacks.navigation.forkNativeSession}
          onForkTask={callbacks.navigation.forkTask}
          onLoadNativeSessions={callbacks.navigation.loadNativeSessions}
          onManageWorktrees={manageWorktrees}
          onRemoveProject={prepareProjectRemoval}
          onRenameProject={async (projectId, label) => { await controller.intents.projects.rename(projectId, label); }}
          onNewTask={(projectId) => {
            // A native shell opens New Task in a fresh webview client, so carry
            // this client's retained selection across that shell boundary.
            const handoffProjectId = projectId ?? navigation.newTaskSelection.projectId;
            callbacks.navigation.openNewTask(handoffProjectId);
          }}
          onOpenNativeSession={callbacks.navigation.openNativeSession}
          onOpenWorkspaceFolder={controller.workspaceSetup?.openFolder}
          onOpenTask={callbacks.navigation.openTask}
          onRecoverNativeSessions={(kind) => kind === "launchFailed"
            ? callbacks.navigation.loadNativeSessions()
            : callbacks.navigation.openSettings()}
          onRestoreTask={callbacks.navigation.restoreTask}
          onSetTaskPinned={callbacks.navigation.setTaskPinned}
          onSetTaskTitle={callbacks.navigation.setTaskTitle}
          onRestoreNativeSession={callbacks.navigation.restoreNativeSession}
          onSetNativeSessionPinned={callbacks.navigation.setNativeSessionPinned}
          onSetNativeSessionTitle={callbacks.navigation.setNativeSessionTitle}
          onSearchChange={callbacks.navigation.changeSearch}
          onSettings={() => callbacks.navigation.openSettings()}
          onToggleArchived={callbacks.navigation.toggleArchived}
          projects={navigationProjects}
          searchQuery={navigation.searchQuery}
          showArchived={navigation.showArchived}
          taskListError={navigation.taskListError}
          tasks={visibleTasks}
        />
        {projectFolderDialog}
        {projectRemoveDialog}
      </main>
    );
  }

  if (appServerError && !isWebShell) {
    return (
      <main className={`app-shell editor-shell ${desktopTitleBar ? "desktop-error-shell" : ""}`}>
        {desktopTitleBar}
        <AppServerErrorView message={appServerError} />
      </main>
    );
  }

  if (bootstrap.surface === "settings") {
    return (
      <SettingsView
        backendConnection={controller.backendConnection}
        developerSettingsUnlocked={bootstrap.developerSettingsUnlocked}
        desktopNotifications={taskNotifications?.settings}
        frameHeader={desktopSettingsTitleBar}
        frameHeaderPlacement={desktopSettingsTitleBarPlacement}
        onAuthenticate={authenticateAndReturn}
        onBackToApp={backFromSettings}
        onCreateCustomAgent={callbacks.settings.createCustomAgent}
        onDeleteCustomAgent={callbacks.settings.deleteCustomAgent}
        onDeleteMcpServer={callbacks.settings.deleteMcpServer}
        onGetMcpServerDetails={callbacks.settings.getMcpServerDetails}
        onGetSkillDetails={callbacks.settings.getSkillDetails}
        onNewTaskInWorktree={newTaskInWorktree}
        onRefresh={callbacks.settings.refreshSettings}
        onReplaceCustomAgent={callbacks.settings.replaceCustomAgent}
        onResetTaskHistory={callbacks.settings.resetTaskHistory}
        onSelectTab={callbacks.settings.selectSettingsTab}
        onSetDesktopNotifications={taskNotifications?.setEnabled}
        onSetAcpTrace={callbacks.settings.setAcpTrace}
        onSetAgentEnabled={callbacks.settings.setAgentEnabled}
        onSetMcpServerEnabled={callbacks.settings.setMcpServerEnabled}
        onSaveMcpServer={callbacks.settings.saveMcpServer}
        onSetComposerSubmitShortcut={callbacks.settings.setComposerSubmitShortcut}
        onUpdateCustomAgentMetadata={callbacks.settings.updateCustomAgentMetadata}
        onUnlockDeveloperSettings={callbacks.settings.unlockDeveloperSettings}
        preferences={preferences}
        preferredAgentId={bootstrap.settingsAgentId}
        projects={navigationProjects}
        recoveryActions={settingsRecoveryActions}
        state={settings}
        worktreeIntents={controller.intents.newTask}
        worktreeRepositories={view.primaryTask.newTask.worktreeRepositories}
      />
    );
  }

  if (isProjectWorkbench) {
    const routedActiveTask = bootstrap.taskId ? activeTask : undefined;
    const mobileTitle = renderableTaskSnapshot?.task.title
      ?? routedActiveTask?.title
      ?? (openingNativeSession ? "Opening session" : bootstrap.taskId ? "Opening task" : "New task");
    const mobileProject = activeTask?.project_label ?? navigation.projects[0]?.label ?? "OpenAIDE";
    const mobileTaskStatus = renderableTaskSnapshot?.task.status ?? routedActiveTask?.status;
    const mobileSubtitle = [
      mobileTaskStatus ? taskStatusLabel(mobileTaskStatus) : undefined,
      mobileProject,
    ].filter(Boolean).join(" · ");
    const closeAfter = <T extends unknown[]>(callback: (...args: T) => void) => (...args: T) => {
      closeMobileNavigation();
      callback(...args);
    };
    const openNewTaskFromNavigation = (projectId?: string) => {
      closeMobileNavigation({ restoreFocus: false });
      requestNewTaskFocus();
      callbacks.navigation.openNewTask(projectId);
    };
    const taskNavigation = (
      <Sidebar
        activeTaskId={sidebarActiveTaskId}
        groupByProject={true}
        hiddenFromAccessibility={mobileLayoutActive && !mobileNavigation.active}
        modal={mobileLayoutActive && mobileNavigation.active}
        loadingTasks={!backendReady}
        nativeSessions={navigation.nativeSessions}
        nativeSessionMutations={navigation.nativeSessionMutations}
        nativeSessionAgentId={navigation.newTaskSelection.agentId}
        nativeSessionAgentName={navigation.newTaskSelection.agentLabel}
        nativeSessionProjectId={navigation.newTaskSelection.projectId}
        forkableAgentIds={forkableAgentIds}
        environmentLabel={desktopEnvironmentLabel}
        onArchiveNativeSession={callbacks.navigation.archiveNativeSession}
        onForkNativeSession={callbacks.navigation.forkNativeSession}
        onForkTask={callbacks.navigation.forkTask}
        onAddProject={addProject}
        onArchiveTask={callbacks.navigation.archiveTask}
        onArchiveOlderTasks={callbacks.navigation.archiveOlderTasks}
        onLoadNativeSessions={callbacks.navigation.loadNativeSessions}
        onManageWorktrees={(projectId) => { closeMobileNavigation({ restoreFocus: false }); manageWorktrees(projectId); }}
        onRemoveProject={(project) => { closeMobileNavigation({ restoreFocus: false }); prepareProjectRemoval(project); }}
        onRenameProject={async (projectId, label) => { await controller.intents.projects.rename(projectId, label); }}
        onNewTask={openNewTaskFromNavigation}
        onOpenNativeSession={closeAfter(callbacks.navigation.openNativeSession)}
        onOpenTask={closeAfter(callbacks.navigation.openTask)}
        onRecoverNativeSessions={(kind) => kind === "launchFailed"
          ? callbacks.navigation.loadNativeSessions()
          : callbacks.navigation.openSettings()}
        onRestoreTask={callbacks.navigation.restoreTask}
        onSetTaskPinned={callbacks.navigation.setTaskPinned}
        onSetTaskTitle={callbacks.navigation.setTaskTitle}
        onRestoreNativeSession={callbacks.navigation.restoreNativeSession}
        onSetNativeSessionPinned={callbacks.navigation.setNativeSessionPinned}
        onSetNativeSessionTitle={callbacks.navigation.setNativeSessionTitle}
        onSearchChange={callbacks.navigation.changeSearch}
        onSettings={closeAfter(() => callbacks.navigation.openSettings())}
        onToggleArchived={callbacks.navigation.toggleArchived}
        projects={navigation.projects}
        searchQuery={navigation.searchQuery}
        showArchived={navigation.showArchived}
        taskListError={navigation.taskListError}
        tasks={visibleTasks}
      />
    );
    return (
      <AppSidebarFrame
        className={[
          "app-shell web-workbench-shell",
          mobileNavigationOpen ? "mobile-navigation-open" : undefined,
          mobileNavigation.dragging ? "mobile-navigation-dragging" : undefined,
        ].filter(Boolean).join(" ")}
        header={desktopTitleBar}
        headerPlacement={desktopTitleBarPlacement}
        onKeyDown={trapMobileNavigationFocus}
        onPointerCancel={mobileNavigation.cancelSwipe}
        onPointerDownCapture={mobileNavigation.beginSwipe}
        onPointerMoveCapture={mobileNavigation.trackSwipe}
        onPointerUp={mobileNavigation.endSwipe}
        sidebar={taskNavigation}
        style={mobileNavigation.dragProgress === undefined
          ? undefined
          : { "--mobile-navigation-progress": mobileNavigation.dragProgress } as CSSProperties}
      >
         {projectFolderDialog}
         {projectRemoveDialog}
        <header
          className="mobile-workbench-bar"
          data-has-permission-policy={hasMobilePermissionPolicy || undefined}
          data-has-plan={renderableTaskSnapshot?.current_plan ? true : undefined}
        >
          <button
            aria-expanded={mobileNavigationOpen}
            aria-label={mobileNavigationOpen ? "Close task navigation" : "Open task navigation"}
            onClick={() => {
              if (mobileNavigationOpen) {
                closeMobileNavigation();
                return;
              }
              setPlanDrawerOpen(false);
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
          {mobilePermissionTask ? (
            <TaskPermissionPolicyControl
              disabled={renderableTaskArchived || !taskMutationReady}
              disabledReason={renderableTaskArchived
                ? "Archived Tasks are read-only."
                : !taskMutationReady
                  ? "Permission handling is unavailable until this Task is connected."
                  : undefined}
              onChange={callbacks.task.setPermissionPolicy}
              policy={mobilePermissionTask.permission_policy}
            />
          ) : null}
          {renderableTaskSnapshot?.current_plan ? (
            <button
              aria-expanded={planDrawerOpen}
              aria-label={planDrawerOpen ? "Hide Plan" : "Open Plan"}
              className="task-plan-appear-chip"
              onClick={() => {
                closeMobileNavigation({ restoreFocus: false });
                setPlanDrawerOpen((open) => !open);
              }}
              type="button"
            >
              {planDrawerOpen ? <X size={17} /> : <ListTodo size={17} />}
            </button>
          ) : null}
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
          ) : (
            <AppPrimaryTaskSurface
              controller={controller}
              focusRequestKey={newTaskFocusRequestKey}
              model={taskSurfaceModel}
              projects={navigationProjects}
              onAddProject={addProject}
              onCheckProject={async (projectId) => { await controller.intents.projects.refresh(projectId); }}
              onRemoveProject={(projectId) => {
                const project = navigation.projects.find((candidate) => candidate.projectId === projectId);
                if (project) prepareProjectRemoval({
                  projectId,
                  label: project.label,
                  ...projectRemovalCounts(projectId),
                });
               }}
              onPlanDrawerOpenChange={setPlanDrawerOpen}
              planDrawerOpen={planDrawerOpen}
              workspaceRecovery={{ manageWorktrees, openProjectSettings: callbacks.navigation.openSettings, reconnectProject: callbacks.navigation.openNewTask }}
            />
          )}
        </section>
      </AppSidebarFrame>
    );
  }

  return (
    <main className="app-shell editor-shell">
      <AppPrimaryTaskSurface
        controller={controller}
        focusRequestKey={newTaskFocusRequestKey}
        model={taskSurfaceModel}
        projects={navigationProjects}
        onAddProject={addProject}
        onCheckProject={async (projectId) => { await controller.intents.projects.refresh(projectId); }}
        onRemoveProject={(projectId) => {
          const project = navigation.projects.find((candidate) => candidate.projectId === projectId);
          if (project) prepareProjectRemoval({
            projectId,
            label: project.label,
            ...projectRemovalCounts(projectId),
          });
         }}
        onPlanDrawerOpenChange={setPlanDrawerOpen}
        planDrawerOpen={planDrawerOpen}
        workspaceRecovery={{ manageWorktrees, openProjectSettings: callbacks.navigation.openSettings, reconnectProject: callbacks.navigation.openNewTask }}
      />
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
