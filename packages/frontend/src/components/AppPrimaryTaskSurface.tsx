import { NewTaskView } from "./NewTaskView";
import { TaskLoadingView, TaskView } from "./TaskView";
import type { AppController } from "./appController";

export function primaryTaskSurfaceModel(controller: AppController) {
  const { activeTask, bootstrap, state } = controller;
  const snapshotTaskInput = state.snapshot ? state.taskInputs[state.snapshot.task.task_id] : undefined;
  const renderableTaskSnapshot = state.snapshot?.task.has_messages === true
    || (bootstrap.surface === "task" && bootstrap.taskId && snapshotTaskInput?.pending)
    ? state.snapshot
    : undefined;
  const startupConfigOptions = renderableTaskSnapshot?.task.has_messages === false && snapshotTaskInput?.pending
    ? state.newTask.pending?.configOptions
    : undefined;
  const openingNativeSession = state.newTask.nativeSessions.adoptingSessionId !== undefined;
  const renderableTaskArchived = Boolean(
    state.showArchived
      && renderableTaskSnapshot
      && activeTask?.task_id === renderableTaskSnapshot.task.task_id,
  );
  const taskLoadingError = bootstrap.taskId && state.taskOpenError?.taskId === bootstrap.taskId
    ? state.taskOpenError.message
    : undefined;
  return {
    openingNativeSession,
    renderableTaskArchived,
    renderableTaskSnapshot,
    startupConfigOptions,
    taskLoadingError,
  };
}

type AppPrimaryTaskSurfaceProps = {
  controller: AppController;
  focusRequestKey: number;
  model: ReturnType<typeof primaryTaskSurfaceModel>;
};

export function AppPrimaryTaskSurface({ controller, focusRequestKey, model }: AppPrimaryTaskSurfaceProps) {
  const { activeTask, agents, backendReady, bootstrap, callbacks, dispatch, preferences, state } = controller;
  const {
    openingNativeSession,
    renderableTaskArchived,
    renderableTaskSnapshot,
    startupConfigOptions,
    taskLoadingError,
  } = model;
  const isWebShell = bootstrap.surface !== "invalid" && bootstrap.appServerConnection?.kind === "webProxy";
  const retryTaskOpen = taskLoadingError || controller.backendConnectionState.status === "unavailable"
    ? controller.retryTaskOpen
    : undefined;

  if (renderableTaskSnapshot && !openingNativeSession) {
    return (
      <TaskView
        activeTask={activeTask}
        archived={renderableTaskArchived}
        backendConnectionState={controller.backendConnectionState}
        chatPageState={state.chatPages[renderableTaskSnapshot.task.task_id]}
        backendReady={backendReady}
        dispatch={dispatch}
        fileBrowser={callbacks.task.fileBrowser}
        onCancel={renderableTaskSnapshot.task.has_messages
          ? callbacks.task.cancel
          : callbacks.newTask.cancel}
        onLoadChatPage={callbacks.task.loadChatPage}
        onLoadToolDetail={callbacks.task.loadToolDetail}
        onPermissionRespond={callbacks.task.respondToPermission}
        onQuestionRespond={callbacks.task.respondToQuestion}
        onRetryConnection={retryTaskOpen}
        onRevealAttachment={callbacks.task.revealAttachment}
        onRemoveAttachment={callbacks.task.removeAttachment}
        onRetryHistory={callbacks.task.retryHistory}
        onRestoreTask={callbacks.navigation.restoreTask}
        onSelectConfigOption={callbacks.task.selectConfigOption}
        onSendPrompt={callbacks.task.sendPrompt}
        appServerPermissionRequests={state.appServerPermissionRequests}
        appServerQuestionRequests={state.appServerQuestionRequests}
        permissionResponses={state.permissionResponses}
        questionResponses={state.questionResponses}
        savedScrollTop={state.taskScrollPositions[renderableTaskSnapshot.task.task_id]}
        snapshot={renderableTaskSnapshot}
        startupConfigOptions={startupConfigOptions}
        submitShortcut={preferences.composer_submit_shortcut}
        taskInput={state.taskInputs[renderableTaskSnapshot.task.task_id] ?? { prompt: "", context: [] }}
        toolDetails={state.toolDetails}
        showWorkspaceContext={isWebShell}
      />
    );
  }

  if (bootstrap.taskId || openingNativeSession) {
    return (
      <TaskLoadingView
        error={taskLoadingError}
        onRetry={retryTaskOpen}
      />
    );
  }

  return (
    <NewTaskView
      agents={agents}
      dispatch={dispatch}
      fileBrowser={callbacks.newTask.fileBrowser}
      focusRequestKey={focusRequestKey}
      loadingProjects={!backendReady}
      onCancelTask={callbacks.newTask.cancel}
      onSelectConfigOption={callbacks.newTask.selectConfigOption}
      onSubmitTask={callbacks.newTask.submit}
      projectContextMode={isWebShell ? "selectable" : "fixed"}
      resetOptionsRequestKey={callbacks.newTask.resetOptionsRequestKey}
      state={state}
      submitShortcut={preferences.composer_submit_shortcut}
      workspaceBrowser={callbacks.newTask.workspaceBrowser}
    />
  );
}
