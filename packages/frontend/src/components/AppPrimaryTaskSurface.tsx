import { NewTaskView } from "./NewTaskView";
import { TaskLoadingView, TaskView } from "./TaskView";
import type { AppController } from "./appController";
import { openRecoveryUrl, reloadRecoveryShell } from "../services/hostBridge";
import type { AgentRecoveryActions } from "./AgentRecovery";

export function primaryTaskSurfaceModel(controller: AppController) {
  const { activeTask, bootstrap, view } = controller;
  const { primaryTask } = view;
  const snapshotTaskInput = primaryTask.taskInput;
  const adoptedEmptyTaskHasDraft = bootstrap.surface === "task"
    && bootstrap.taskId === primaryTask.snapshot?.task.task_id
    && hasVisibleTaskDraft(snapshotTaskInput);
  // Task preparation can publish an active New Task while the route remains
  // /new-task. Only an explicit Task route may promote that snapshot to TaskView.
  const activeNoMessageTask = bootstrap.taskId === primaryTask.snapshot?.task.task_id
    && primaryTask.snapshot?.task.status === "active";
  const renderableTaskSnapshot = primaryTask.snapshot?.task.has_messages === true
    || adoptedEmptyTaskHasDraft
    || activeNoMessageTask
    ? primaryTask.snapshot
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
  const taskLoadingError = openingNativeSession
    ? nativeRouteError
    : bootstrap.taskId && primaryTask.taskOpenError?.taskId === bootstrap.taskId
      ? primaryTask.taskOpenError.message
      : undefined;
  return {
    openingNativeSession,
    renderableTaskArchived,
    renderableTaskSnapshot,
    startupConfigOptions,
    taskLoadingError,
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
  workspaceRecovery?: {
    manageWorktrees: (projectId: string) => void;
    openProjectSettings: () => void;
    reconnectProject: (projectId: string) => void;
  };
};

export function AppPrimaryTaskSurface({ controller, focusRequestKey, model, workspaceRecovery }: AppPrimaryTaskSurfaceProps) {
  const { activeTask, agents, backendReady, bootstrap, callbacks, intents, preferences, view } = controller;
  const { primaryTask } = view;
  const {
    openingNativeSession,
    renderableTaskArchived,
    renderableTaskSnapshot,
    startupConfigOptions,
    taskLoadingError,
  } = model;
  const usesProjectNavigation = bootstrap.surface !== "invalid" && bootstrap.shell.navigationMode === "project";
  const retryTaskOpen = !openingNativeSession
    && (taskLoadingError || controller.backendConnectionState.status === "unavailable")
    ? controller.retryTaskOpen
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
        intents={intents.task}
        onCancel={renderableTaskSnapshot.task.has_messages || renderableTaskSnapshot.task.status === "active"
          ? callbacks.task.cancel
          : callbacks.newTask.cancel}
        onLoadChatPage={callbacks.task.loadChatPage}
        onManageWorktrees={workspaceRecovery?.manageWorktrees}
        onOpenProjectSettings={workspaceRecovery?.openProjectSettings}
        onSubscribeToolDetail={callbacks.task.subscribeToolDetail}
        onPermissionRespond={callbacks.task.respondToPermission}
        onQuestionRespond={callbacks.task.respondToQuestion}
        onReconnectProject={workspaceRecovery?.reconnectProject}
        onRetryConnection={retryTaskOpen}
        onRevealAttachment={callbacks.task.revealAttachment}
        onRemoveAttachment={callbacks.task.removeAttachment}
        onRestoreTask={callbacks.navigation.restoreTask}
        onSelectConfigOption={callbacks.task.selectConfigOption}
        onSendPrompt={callbacks.task.sendPrompt}
        permissionResponses={primaryTask.permissionResponses}
        liveTextPresentation={primaryTask.liveTextPresentation}
        questionResponses={primaryTask.questionResponses}
        savedScrollState={primaryTask.savedScrollState}
        snapshot={renderableTaskSnapshot}
        startupConfigOptions={startupConfigOptions}
        submitShortcut={preferences.composer_submit_shortcut}
        taskInput={primaryTask.taskInput ?? { prompt: "", context: [] }}
        toolDetails={primaryTask.toolDetails}
        showWorkspaceContext={usesProjectNavigation}
      />
    );
  }

  if (bootstrap.taskId || openingNativeSession) {
    return (
      <TaskLoadingView
        error={taskLoadingError}
        label={openingNativeSession ? "Opening session" : undefined}
        onRetry={retryTaskOpen}
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
      onRemoveAttachment={callbacks.newTask.removeAttachment}
      onSelectConfigOption={callbacks.newTask.selectConfigOption}
      onSubmitTask={callbacks.newTask.submit}
      projectContextMode={usesProjectNavigation ? "selectable" : "fixed"}
      state={primaryTask.newTask}
      submitShortcut={preferences.composer_submit_shortcut}
      workspaceBrowser={callbacks.newTask.workspaceBrowser}
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
