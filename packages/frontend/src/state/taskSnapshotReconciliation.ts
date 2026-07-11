import type { TaskSnapshot, TaskSummary } from "@openaide/app-shell-contracts";
import type { ComposerAttachment } from "./composerOptions";
import type { AppState } from "./store";

export type PendingInputReconciliation = ReturnType<typeof pendingInputReconciliation>;

export function pendingInputReconciliation(state: AppState, snapshot: TaskSnapshot) {
  const input = state.taskInputs[snapshot.task.task_id];
  const taskInputCommitted = input?.pending
    ? snapshotContainsPendingInput(snapshot, input.pending)
    : false;
  const taskInputRestoredSendCommitted = input && !input.pending && input.error
    ? snapshotContainsPendingInput(snapshot, input)
    : false;
  const newTaskCommitted = state.newTask.pending
    ? snapshotContainsPendingInput(snapshot, state.newTask.pending)
    : false;
  return { taskInputCommitted, taskInputRestoredSendCommitted, newTaskCommitted };
}

export function applyPendingInputReconciliation(
  state: AppState,
  taskId: string,
  reconciliation: PendingInputReconciliation,
) {
  if (
    !reconciliation.taskInputCommitted
    && !reconciliation.taskInputRestoredSendCommitted
    && !reconciliation.newTaskCommitted
  ) {
    return state;
  }
  const input = state.taskInputs[taskId];
  return {
    ...state,
    taskInputs: input && (reconciliation.taskInputCommitted || reconciliation.taskInputRestoredSendCommitted)
      ? {
          ...state.taskInputs,
          [taskId]: reconciliation.taskInputRestoredSendCommitted
            ? { prompt: "", context: [] }
            : { prompt: input.prompt, context: input.context },
        }
      : state.taskInputs,
    newTask: reconciliation.newTaskCommitted
      ? {
          ...state.newTask,
          prompt: "",
          pending: undefined,
          submitting: false,
          error: undefined,
        }
      : state.newTask,
  };
}

export function reconcileBackgroundTaskSnapshot(state: AppState, snapshot: TaskSnapshot): AppState {
  const taskId = snapshot.task.task_id;
  const reconciliation = pendingInputReconciliation(state, snapshot);
  const current = state.taskSnapshots[taskId];
  if (shouldIgnoreStaleTaskSnapshot(current, snapshot)) {
    return applyPendingInputReconciliation(state, taskId, reconciliation);
  }
  const tasks = upsertTaskSummary(state.tasks, snapshot.task);
  const reconciled = applyPendingInputReconciliation(state, taskId, reconciliation);
  return {
    ...reconciled,
    tasks,
    taskSnapshots: {
      ...state.taskSnapshots,
      [taskId]: snapshot,
    },
    taskListCache: {
      ...state.taskListCache,
      [state.showArchived ? "archived" : "active"]: tasks,
    },
  };
}

export function reconcileTaskNavigationTasks(state: AppState, incoming: TaskSummary[]) {
  if (state.showArchived) return incoming;
  const incomingIds = new Set(incoming.map((task) => task.task_id));
  const locallyPending = state.tasks.filter((task) =>
    !incomingIds.has(task.task_id) && state.taskInputs[task.task_id]?.pending !== undefined
  );
  return locallyPending.length ? [...incoming, ...locallyPending] : incoming;
}

export function shouldIgnoreStaleTaskSnapshot(current: TaskSnapshot | undefined, incoming: TaskSnapshot) {
  if (!current || current.task.task_id !== incoming.task.task_id) return false;
  if (incoming.revision < current.revision) return true;
  if (incoming.revision > current.revision) return false;
  if (incoming.task.message_history_version < current.task.message_history_version) return true;
  if (incoming.chat.version < current.chat.version) return true;
  if (current.task.has_messages && !incoming.task.has_messages) return true;
  if (current.chat.items.length > incoming.chat.items.length) return true;
  return false;
}

export function upsertTaskSummary(tasks: TaskSummary[], task: TaskSummary) {
  const index = tasks.findIndex((item) => item.task_id === task.task_id);
  if (index === -1) return [task, ...tasks];
  return [
    ...tasks.slice(0, index),
    task,
    ...tasks.slice(index + 1),
  ];
}

function snapshotContainsPendingInput(
  snapshot: TaskSnapshot,
  pending: { prompt: string; context: ComposerAttachment[] },
) {
  return snapshot.chat.items.some((item) => {
    if (item.message.kind !== "user") return false;
    if (item.message.text !== pending.prompt) return false;
    return (item.message.attachments?.length ?? 0) === pending.context.length;
  });
}
