import { Menu, X } from "lucide-react";
import { useEffect, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { AppPrimaryTaskSurface, primaryTaskSurfaceModel } from "./AppPrimaryTaskSurface";
import { DEFAULT_MAX_TASKS_PER_PROJECT, Sidebar } from "./Sidebar";
import { SettingsView } from "./settings/SettingsView";
import { taskStatusLabel } from "./TaskHeader";
import type { AppController } from "./appController";
import { useMobileNavigation } from "./useMobileNavigation";
import { useInputModality } from "./useInputModality";

export function AppSurfaces({ controller }: { controller: AppController }) {
  useInputModality();
  const { activeNavigationTaskId, activeTask, backendReady, bootstrap, callbacks, preferences, state, visibleTasks } = controller;
  const [mobileLayoutActive, setMobileLayoutActive] = useState(() => isMobileWebViewport());
  const [newTaskFocusRequestKey, setNewTaskFocusRequestKey] = useState(0);
  const mobileNavigationButtonRef = useRef<HTMLButtonElement | null>(null);
  const webMainSurfaceRef = useRef<HTMLElement | null>(null);
  const isWebShell = bootstrap.surface !== "invalid" && bootstrap.appServerConnection?.kind === "webProxy";
  const isWebWorkbench = isWebShell && (bootstrap.surface === "task" || bootstrap.surface === "settings");
  const mobileNavigation = useMobileNavigation(isWebWorkbench && mobileLayoutActive);
  const mobileNavigationOpen = mobileNavigation.open;
  const taskSurfaceModel = primaryTaskSurfaceModel(controller);
  const { openingNativeSession, renderableTaskSnapshot } = taskSurfaceModel;
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
          nativeSessions={state.newTask.nativeSessions}
          nativeSessionAgentId={state.newTask.selection.agentId}
          nativeSessionAgentName={state.newTask.selection.agentLabel}
          nativeSessionProjectId={state.newTask.selection.projectId}
          onArchiveTask={callbacks.navigation.archiveTask}
          onLoadNativeSessions={callbacks.navigation.loadNativeSessions}
          onNewTask={callbacks.navigation.openNewTask}
          onOpenNativeSession={callbacks.navigation.openNativeSession}
          onOpenTask={callbacks.navigation.openTask}
          onRestoreTask={callbacks.navigation.restoreTask}
          onSearchChange={callbacks.navigation.changeSearch}
          onSettings={callbacks.navigation.openSettings}
          onToggleArchived={callbacks.navigation.toggleArchived}
          searchQuery={state.searchQuery}
          showArchived={state.showArchived}
          taskListError={state.taskListError}
          tasks={visibleTasks}
        />
      </main>
    );
  }

  if (state.appServerError && !isWebShell) {
    return (
      <main className="app-shell editor-shell">
        <AppServerErrorView message={state.appServerError} />
      </main>
    );
  }

  if (bootstrap.surface === "settings" && !isWebShell) {
    return (
      <main className="app-shell editor-shell">
        <SettingsView
          onAuthenticate={callbacks.settings.authenticateAgent}
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
          state={state.settings}
        />
      </main>
    );
  }

  if (isWebWorkbench) {
    const mobileTitle = bootstrap.surface === "settings"
      ? "Settings"
      : renderableTaskSnapshot?.task.title ?? activeTask?.title ?? (bootstrap.taskId || openingNativeSession ? "Opening task" : "New task");
    const mobileProject = activeTask?.project_label ?? state.projects[0]?.label ?? "OpenAIDE";
    const mobileTaskStatus = renderableTaskSnapshot?.task.status ?? activeTask?.status;
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
          {state.appServerError ? (
            <AppServerErrorView message={state.appServerError} />
          ) : bootstrap.surface === "settings" ? (
            <SettingsView
              onAuthenticate={callbacks.settings.authenticateAgent}
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
              state={state.settings}
            />
          ) : (
            <AppPrimaryTaskSurface
              controller={controller}
              focusRequestKey={newTaskFocusRequestKey}
              model={taskSurfaceModel}
            />
          )}
        </section>
        <Sidebar
          activeTaskId={bootstrap.surface === "settings" ? undefined : activeNavigationTaskId}
          groupByProject
          hiddenFromAccessibility={mobileLayoutActive && !mobileNavigation.active}
          maxTasksPerProject={DEFAULT_MAX_TASKS_PER_PROJECT}
          modal={mobileLayoutActive && mobileNavigation.active}
          loadingTasks={!backendReady}
          nativeSessions={state.newTask.nativeSessions}
          nativeSessionAgentId={state.newTask.selection.agentId}
          nativeSessionAgentName={state.newTask.selection.agentLabel}
          nativeSessionProjectId={state.newTask.selection.projectId}
          onArchiveTask={callbacks.navigation.archiveTask}
          onLoadNativeSessions={callbacks.navigation.loadNativeSessions}
          onNewTask={openNewTaskFromNavigation}
          onOpenNativeSession={closeAfter(callbacks.navigation.openNativeSession)}
          onOpenTask={closeAfter(callbacks.navigation.openTask)}
          onRestoreTask={callbacks.navigation.restoreTask}
          onSearchChange={callbacks.navigation.changeSearch}
          onSettings={closeAfter(callbacks.navigation.openSettings)}
          onToggleArchived={callbacks.navigation.toggleArchived}
          projects={state.projects}
          searchQuery={state.searchQuery}
          settingsActive={bootstrap.surface === "settings"}
          showArchived={state.showArchived}
          taskListError={state.taskListError}
          tasks={visibleTasks}
        />
      </main>
    );
  }

  return (
    <main className="app-shell editor-shell">
      <AppPrimaryTaskSurface
        controller={controller}
        focusRequestKey={newTaskFocusRequestKey}
        model={taskSurfaceModel}
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
