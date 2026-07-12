import { invalidateAppServerAttachments } from "./composerOptions";
import { createInitialState, type AppState } from "./store";

const RESTART_ATTACHMENT_MESSAGE = "Attachment must be reselected after App Server restart.";
const RESTART_SEND_MESSAGE = "Send status is unknown after App Server restart. Retry sends this exact message.";

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
    taskOpenError: undefined,
    appServerPermissionRequests: initial.appServerPermissionRequests,
    appServerQuestionRequests: initial.appServerQuestionRequests,
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
        // The exact idempotent send keeps its opaque handles until recovery proves
        // whether the old process consumed them. The draft remains locked meanwhile.
        return [taskId, {
          ...input,
          error: RESTART_SEND_MESSAGE,
          pending: { ...input.pending, state: "uncertain" as const },
        }];
      }
      return [taskId, {
        ...input,
        context: invalidateAppServerAttachments(input.context, RESTART_ATTACHMENT_MESSAGE),
      }];
    }),
  ) as AppState["taskInputs"];
  const protectedNewTaskSend = state.newTask.pending?.idempotencyKey !== undefined;
  const newTaskContext = protectedNewTaskSend
    ? state.newTask.context
    : invalidateAppServerAttachments(state.newTask.context, RESTART_ATTACHMENT_MESSAGE);
  const newTaskPending = state.newTask.pending
    ? {
        ...state.newTask.pending,
        context: protectedNewTaskSend
          ? state.newTask.pending.context
          : invalidateAppServerAttachments(state.newTask.pending.context, RESTART_ATTACHMENT_MESSAGE),
      }
    : undefined;
  const newTaskWasBusy = state.newTask.submitting
    || state.newTask.nativeSessions.adoptingSessionId !== undefined;

  return {
    ...state,
    appServerPermissionRequests: {},
    appServerQuestionRequests: {},
    permissionResponses: {},
    questionResponses: {},
    taskInputs,
    toolDetails: {},
    newTask: {
      ...state.newTask,
      context: newTaskContext,
      pending: newTaskPending,
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
