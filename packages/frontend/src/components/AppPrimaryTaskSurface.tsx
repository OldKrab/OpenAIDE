import { NewTaskView } from "./NewTaskView";
import { TaskLoadingView, TaskView } from "./TaskView";
import type { AppController } from "./appController";
import { openRecoveryUrl, reloadRecoveryShell } from "../services/hostBridge";
import type { AgentRecoveryActions } from "./AgentRecovery";

export function primaryTaskSurfaceModel(controller: AppController) {
  const { activeTask, bootstrap, view } = controller;
  const { primaryTask } = view;
  const snapshotTaskInput = primaryTask.taskInput;
  // Chat belongs to one Task identity. A leftover replica from reconnect or
  // navigation must not paint another Task's history onto the current route.
  const routedSnapshot = bootstrap.surface === "task"
    && bootstrap.taskId
    && primaryTask.snapshot?.task.task_id === bootstrap.taskId
    ? primaryTask.snapshot
    : undefined;
  const adoptedEmptyTaskHasDraft = Boolean(routedSnapshot) && hasVisibleTaskDraft(snapshotTaskInput);
  // Task preparation can publish an active New Task while the route remains
  // /new-task. Only an explicit Task route may promote that snapshot to TaskView.
  const activeNoMessageTask = routedSnapshot?.task.status === "active";
  const renderableTaskSnapshot = routedSnapshot
    && (
      routedSnapshot.task.has_messages === true
      || adoptedEmptyTaskHasDraft
      || activeNoMessageTask
    )
    ? routedSnapshot
    : undefined;
  const startupConfigOptions = renderableTaskSnapshot?.task.has_messages === false && snapshotTaskInput?.pending
    ? primaryTask.newTask.newTask.pending?.configOptions
    : undefined;
  const openingNativeSession = bootstrap.surface === "nativeSession";
  const renderableTaskArchived = Boolean(
    view.navigation.showArchived
      && renderableTaskSnapshot
      && activeTask?.task_id === renderableTaskSnapshot.task.task_id,
  );
  const adoptionError = primaryTask.newTask.newTask.nativeSessions.adoptionError;
  const routedNativeSessionId = bootstrap.surface === "nativeSession" ? bootstrap.nativeSessionId : undefined;
  const nativeRouteError = adoptionError && adoptionError.sessionId === routedNativeSessionId
    ? adoptionError.message
    : undefined;
  const nativeRouteRetryable = adoptionError?.sessionId === routedNativeSessionId
    && adoptionError?.recoverable === true;
  const routedTaskOpenError = bootstrap.taskId
    && primaryTask.taskOpenError?.taskId === bootstrap.taskId
    ? primaryTask.taskOpenError
    : undefined;
  const taskLoadingError = openingNativeSession ? nativeRouteError : routedTaskOpenError?.message;
  const taskLoadingErrorKind = openingNativeSession
    ? nativeRouteError ? adoptionError?.kind ?? "failed" as const : undefined
    : routedTaskOpenError?.kind;
  return {
    openingNativeSession,
    renderableTaskArchived,
    renderableTaskSnapshot,
    startupConfigOptions,
    taskLoadingError,
    taskLoadingErrorKind,
    nativeRouteRetryable,
  };
}

function hasVisibleTaskDraft(input: AppController["view"]["primaryTask"]["taskInput"]) {
  return Boolean(
    input
    && (
      input.pending !== undefined
      || input.prompt.length !== 0
      || input.context.length !== 0
      || input.error !== undefined
    )
  );
}

type AppPrimaryTaskSurfaceProps = {
  controller: AppController;
  focusRequestKey: number;
  model: ReturnType<typeof primaryTaskSurfaceModel>;
  projects?: AppController["view"]["navigation"]["projects"];
  onPlanDrawerOpenChange?: (open: boolean) => void;
  onAddProject?: () => void;
  onCheckProject?: (projectId: string) => Promise<void>;
  onRemoveProject?: (projectId: string) => void;
  planDrawerOpen?: boolean;
  workspaceRecovery?: {
    manageWorktrees: (projectId: string) => void;
    openProjectSettings: () => void;
    reconnectProject: (projectId: string) => void;
  };
};

export function AppPrimaryTaskSurface({
  controller,
  focusRequestKey,
  model,
  projects,
  onPlanDrawerOpenChange,
  onAddProject,
  onCheckProject,
  onRemoveProject,
  planDrawerOpen,
  workspaceRecovery,
}: AppPrimaryTaskSurfaceProps) {
  const { activeTask, agents, backendReady, bootstrap, callbacks, intents, preferences, view } = controller;
  const { primaryTask } = view;
  const {
    openingNativeSession,
    renderableTaskArchived,
    renderableTaskSnapshot,
    startupConfigOptions,
    taskLoadingError,
    taskLoadingErrorKind,
    nativeRouteRetryable,
  } = model;
  const usesProjectNavigation = bootstrap.surface !== "invalid" && bootstrap.shell.navigationMode === "project";
  const canSelectNewTaskProject = usesProjectNavigation
    || (bootstrap.surface !== "invalid" && (bootstrap.projectIds?.length ?? 0) > 1);
  const retryTaskOpen = !openingNativeSession
    && (taskLoadingError || controller.backendConnectionState.status === "unavailable")
    ? controller.retryTaskOpen
    : undefined;
  const retryNativeSessionOpen = openingNativeSession && nativeRouteRetryable
    ? controller.retryNativeSessionOpen
    : undefined;
  const recoveryActions = createAgentRecoveryActions(controller);

  if (renderableTaskSnapshot && !openingNativeSession) {
    return (
      <TaskView
        activeTask={activeTask}
        agents={agents}
        agentRecoveryActions={recoveryActions}
        archived={renderableTaskArchived}
        backendConnectionState={controller.backendConnectionState}
        chatPageState={primaryTask.chatPageState}
        backendReady={backendReady}
        fileBrowser={callbacks.task.fileBrowser}
        fileViewer={callbacks.task.fileViewer}
        intents={intents.task}
        onCancel={renderableTaskSnapshot.task.has_messages || renderableTaskSnapshot.task.status === "active"
          ? callbacks.task.cancel
          : callbacks.newTask.cancel}
        onClosePlan={callbacks.task.closePlan}
        onAddToQueue={callbacks.task.addToQueue}
        onLoadChatPage={callbacks.task.loadChatPage}
        onLoadComposerHistory={callbacks.task.loadComposerHistory}
        onLoadToolImagePreview={callbacks.task.loadToolImagePreview}
        onManageWorktrees={workspaceRecovery?.manageWorktrees}
        onOpenProjectSettings={workspaceRecovery?.openProjectSettings}
        onSubscribeToolDetail={callbacks.task.subscribeToolDetail}
        onPermissionRespond={callbacks.task.respondToPermission}
        onPermissionPolicyChange={callbacks.task.setPermissionPolicy}
        onQuestionRespond={callbacks.task.respondToQuestion}
        onReconnectProject={workspaceRecovery?.reconnectProject}
        onRetryConnection={retryTaskOpen}
        onRevealAttachment={callbacks.task.revealAttachment}
        onRemoveAttachment={callbacks.task.removeAttachment}
        onRemoveQueueMessage={callbacks.task.removeQueueMessage}
        onReloadNativeSession={callbacks.task.reloadNativeSession}
        onTakeQueueMessage={callbacks.task.takeQueueMessage}
        onMoveQueueMessage={callbacks.task.moveQueueMessage}
        onPlanDrawerOpenChange={onPlanDrawerOpenChange}
        onSendQueueMessageNow={callbacks.task.sendQueueMessageNow}
        onRestoreTask={callbacks.navigation.restoreTask}
        onSelectConfigOption={callbacks.task.selectConfigOption}
        onSendPrompt={callbacks.task.sendPrompt}
        permissionResponses={primaryTask.permissionResponses}
        planDrawerOpen={planDrawerOpen}
        liveTextPresentation={primaryTask.liveTextPresentation}
        questionResponses={primaryTask.questionResponses}
        savedScrollState={primaryTask.savedScrollState}
        snapshot={renderableTaskSnapshot}
        startupConfigOptions={startupConfigOptions}
        submitShortcut={preferences.composer_submit_shortcut}
        taskInput={primaryTask.taskInput ?? { prompt: "", context: [] }}
        toolDetails={primaryTask.toolDetails}
        showWorkspaceContext={usesProjectNavigation && bootstrap.shell.kind !== "desktop"}
      />
    );
  }

  if (bootstrap.taskId || openingNativeSession) {
    return (
      <TaskLoadingView
        error={taskLoadingError}
        errorKind={taskLoadingErrorKind}
        label={openingNativeSession ? "Opening session" : undefined}
        onRetry={retryNativeSessionOpen ?? retryTaskOpen}
      />
    );
  }

  return (
    <NewTaskView
      agents={agents}
      agentRecoveryActions={recoveryActions}
      fileBrowser={callbacks.newTask.fileBrowser}
      focusRequestKey={focusRequestKey}
      intents={intents.newTask}
      loadingProjects={!backendReady}
      onCancelTask={callbacks.newTask.cancel}
      onAddProject={onAddProject}
      onCheckProject={onCheckProject}
      onLoadComposerHistory={callbacks.newTask.loadComposerHistory}
      onManageWorktrees={workspaceRecovery?.manageWorktrees}
      onOpenWorkspaceFolder={controller.workspaceSetup?.openFolder}
      onRemoveAttachment={callbacks.newTask.removeAttachment}
      onRemoveProject={onRemoveProject}
      onSelectConfigOption={callbacks.newTask.selectConfigOption}
      onSubmitTask={callbacks.newTask.submit}
      projectContextMode={canSelectNewTaskProject ? "selectable" : "fixed"}
      state={projects ? { ...primaryTask.newTask, projects } : primaryTask.newTask}
      submitShortcut={preferences.composer_submit_shortcut}
    />
  );
}

/** Creates recovery actions shared by Task and Settings surfaces. */
export function createAgentRecoveryActions(controller: AppController): AgentRecoveryActions {
  const { callbacks, view } = controller;
  return {
    onOpenAgentSettings: (agentId, returnToNewTask) => callbacks.navigation.openSettings(
      agentId,
      returnToNewTask,
      view.primaryTask.newTask.newTask.selection.projectId,
    ),
    onOpenExternal: openRecoveryUrl,
    onReload: reloadRecoveryShell,
    onRetry: callbacks.navigation.retryAgent,
  };
}
