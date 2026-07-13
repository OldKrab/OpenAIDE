import { invalidateAppServerAttachments } from "./composerOptions";
import { createInitialState, type AppState } from "./store";

const RESTART_ATTACHMENT_MESSAGE = "Attachment must be reselected after App Server restart.";

/** Applies the boundary between process-local state and durable state-root data. */
export function applyAppServerReplica(
  state: AppState,
  epoch: number,
  stateRootId: string,
): AppState {
  if (epoch < state.appServerReplicaEpoch) return state;
  const processChanged = state.appServerStateRootId !== undefined
    && state.appServerStateRootId === stateRootId
    && epoch > state.appServerReplicaEpoch;
  const rootChanged = state.appServerStateRootId !== undefined
    && state.appServerStateRootId !== stateRootId;
  if (!rootChanged) {
    const next = {
      ...state,
      appServerError: undefined,
      appServerReplicaEpoch: epoch,
      appServerStateRootId: stateRootId,
    };
    return processChanged ? invalidateProcessOwnedState(next) : next;
  }

  // Revisions and Task IDs are comparable only inside one state root. Keep
  // shell presentation preferences, but drop every cache/handle owned by it.
  const initial = createInitialState();
  return {
    ...state,
    appServerError: undefined,
    appServerReplicaEpoch: epoch,
    appServerStateRootId: stateRootId,
    tasks: initial.tasks,
    taskListCache: initial.taskListCache,
    taskListError: undefined,
    activeTaskId: undefined,
    snapshot: undefined,
    taskSnapshots: initial.taskSnapshots,
    taskSnapshotReplicaEpochs: initial.taskSnapshotReplicaEpochs,
    taskChatScrollStates: initial.taskChatScrollStates,
    taskLiveTextPresentation: initial.taskLiveTextPresentation,
    taskOpenError: undefined,
    permissionResponses: initial.permissionResponses,
    questionResponses: initial.questionResponses,
    projects: initial.projects,
    taskInputs: initial.taskInputs,
    chatPages: initial.chatPages,
    toolDetails: initial.toolDetails,
    settings: initial.settings,
    newTask: initial.newTask,
  };
}

function invalidateProcessOwnedState(state: AppState): AppState {
  const taskInputs = Object.fromEntries(
    Object.entries(state.taskInputs).map(([taskId, input]) => {
      if (input.pending) {
        return [taskId, {
          prompt: input.pending.prompt,
          context: invalidateAppServerAttachments(input.pending.context, RESTART_ATTACHMENT_MESSAGE),
          error: "App Server restarted. Review the draft before sending again.",
        }];
      }
      return [taskId, {
        ...input,
        context: invalidateAppServerAttachments(input.context, RESTART_ATTACHMENT_MESSAGE),
      }];
    }),
  ) as AppState["taskInputs"];
  const newTaskContext = invalidateAppServerAttachments(
    state.newTask.pending?.context ?? state.newTask.context,
    RESTART_ATTACHMENT_MESSAGE,
  );
  const newTaskWasBusy = state.newTask.submitting
    || state.newTask.nativeSessions.adoptingSessionId !== undefined;

  return {
    ...state,
    permissionResponses: {},
    questionResponses: {},
    taskLiveTextPresentation: {},
    taskInputs,
    toolDetails: {},
    newTask: {
      ...state.newTask,
      prompt: state.newTask.pending?.prompt ?? state.newTask.prompt,
      context: newTaskContext,
      pending: undefined,
      submitting: false,
      error: newTaskWasBusy
        ? "App Server restarted. Review the draft and retry."
        : state.newTask.error,
      configOptions: undefined,
      configOptionsLoading: false,
      configOptionsError: undefined,
      selection: {
        ...state.newTask.selection,
        configOptions: {},
      },
      nativeSessions: {
        items: [],
        loading: false,
        loaded: false,
      },
    },
  };
}
