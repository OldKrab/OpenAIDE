import { NewTaskView } from "./NewTaskView";
import { TaskLoadingView, TaskView } from "./TaskView";
import type { AppController } from "./appController";

export function primaryTaskSurfaceModel(controller: AppController) {
  const { activeTask, bootstrap, state } = controller;
  const snapshotTaskInput = state.snapshot ? state.taskInputs[state.snapshot.task.task_id] : undefined;
  const adoptedEmptyTaskHasDraft = bootstrap.surface === "task"
    && bootstrap.taskId === state.snapshot?.task.task_id
    && hasVisibleTaskDraft(snapshotTaskInput);
  // Task preparation can publish an active New Task while the route remains
  // /new-task. Only an explicit Task route may promote that snapshot to TaskView.
  const activeNoMessageTask = bootstrap.taskId === state.snapshot?.task.task_id
    && state.snapshot?.task.status === "active";
  const renderableTaskSnapshot = state.snapshot?.task.has_messages === true
    || adoptedEmptyTaskHasDraft
    || activeNoMessageTask
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

function hasVisibleTaskDraft(input: AppController["state"]["taskInputs"][string] | undefined) {
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
        onCancel={renderableTaskSnapshot.task.has_messages || renderableTaskSnapshot.task.status === "active"
          ? callbacks.task.cancel
          : callbacks.newTask.cancel}
        onLoadChatPage={callbacks.task.loadChatPage}
        onLoadToolDetail={callbacks.task.loadToolDetail}
        onPermissionRespond={callbacks.task.respondToPermission}
        onQuestionRespond={callbacks.task.respondToQuestion}
        onRetryConnection={retryTaskOpen}
        onRevealAttachment={callbacks.task.revealAttachment}
        onRemoveAttachment={callbacks.task.removeAttachment}
        onRestoreTask={callbacks.navigation.restoreTask}
        onSelectConfigOption={callbacks.task.selectConfigOption}
        onSendPrompt={callbacks.task.sendPrompt}
        appServerPermissionRequests={state.appServerPermissionRequests}
        appServerQuestionRequests={state.appServerQuestionRequests}
        permissionResponses={state.permissionResponses}
        liveTextPresentation={state.taskLiveTextPresentation[renderableTaskSnapshot.task.task_id]}
        questionResponses={state.questionResponses}
        savedScrollState={state.taskChatScrollStates[renderableTaskSnapshot.task.task_id]}
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

  const newTaskState = controller.newTaskSnapshot
    ? { ...state, snapshot: controller.newTaskSnapshot }
    : state;

  return (
    <NewTaskView
      agents={agents}
      dispatch={dispatch}
      fileBrowser={callbacks.newTask.fileBrowser}
      focusRequestKey={focusRequestKey}
      loadingProjects={!backendReady}
      onCancelTask={callbacks.newTask.cancel}
      onRemoveAttachment={callbacks.newTask.removeAttachment}
      onSelectConfigOption={callbacks.newTask.selectConfigOption}
      onSubmitTask={callbacks.newTask.submit}
      projectContextMode={isWebShell ? "selectable" : "fixed"}
      resetOptionsRequestKey={callbacks.newTask.resetOptionsRequestKey}
      state={newTaskState}
      submitShortcut={preferences.composer_submit_shortcut}
      workspaceBrowser={callbacks.newTask.workspaceBrowser}
    />
  );
}
