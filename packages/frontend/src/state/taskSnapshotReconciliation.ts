import type { TaskSnapshot, TaskSummary } from "@openaide/app-shell-contracts";
import { retainSnapshotWindow } from "./chatPageMerge";
import type { AppState } from "./store";

export function reconcileBackgroundTaskSnapshot(
  state: AppState,
  incoming: TaskSnapshot,
  replicaEpoch: number,
): AppState {
  const reconciliation = reconcileTaskSnapshotDependents(state, incoming, replicaEpoch);
  if (reconciliation.state === state) return state;
  const { snapshot: reconciled } = reconciliation;
  // Navigation snapshots own membership. A late task/open or mutation response
  // may refresh cached details, but it cannot resurrect or reclassify a Task.
  const tasks = replaceTaskSummary(state.tasks, reconciled.task);
  const activeTasks = replaceTaskSummary(state.taskListCache.active, reconciled.task);
  const archivedTasks = replaceTaskSummary(state.taskListCache.archived, reconciled.task);
  return {
    ...reconciliation.state,
    tasks,
    taskListCache: {
      ...state.taskListCache,
      ...(activeTasks ? { active: activeTasks } : {}),
      ...(archivedTasks ? { archived: archivedTasks } : {}),
    },
  };
}

/**
 * Reconciles one App Server-owned Task snapshot and every Frontend projection
 * whose validity depends on that snapshot, regardless of whether the Task is visible.
 */
export function reconcileTaskSnapshotDependents(
  state: AppState,
  incoming: TaskSnapshot,
  replicaEpoch: number,
): { state: AppState; snapshot: TaskSnapshot } {
  const taskId = incoming.task.task_id;
  const previousSnapshot = state.taskSnapshots[taskId];
  const previousReplicaEpoch = state.taskSnapshotReplicaEpochs[taskId];
  const current = previousReplicaEpoch === replicaEpoch ? previousSnapshot : undefined;
  const snapshot = reconcileTaskSnapshot(current, incoming);
  if (snapshot === current) return { state, snapshot };

  const activePermissionIds = activeRequestIds(snapshot, "permission");
  const activeQuestionIds = activeRequestIds(snapshot, "elicitation");
  const chatPage = retainedChatPage(
    state,
    taskId,
    snapshot,
    previousSnapshot,
    previousReplicaEpoch,
    replicaEpoch,
  );
  return {
    snapshot,
    state: {
      ...state,
      appServerReplicaEpoch: Math.max(state.appServerReplicaEpoch, replicaEpoch),
      taskSnapshots: {
        ...state.taskSnapshots,
        [taskId]: snapshot,
      },
      taskSnapshotReplicaEpochs: {
        ...state.taskSnapshotReplicaEpochs,
        [taskId]: replicaEpoch,
      },
      chatPages: chatPage
        ? { ...state.chatPages, [taskId]: chatPage }
        : omitKeys(state.chatPages, new Set([taskId])),
      permissionResponses: retainKeys(state.permissionResponses, activePermissionIds),
      questionResponses: retainKeys(state.questionResponses, activeQuestionIds),
    },
  };
}

function retainedChatPage(
  state: AppState,
  taskId: string,
  snapshot: TaskSnapshot,
  previousSnapshot: TaskSnapshot | undefined,
  previousReplicaEpoch: number | undefined,
  replicaEpoch: number,
) {
  if (!previousSnapshot) return undefined;
  const previousSyncSnapshot = previousReplicaEpoch === replicaEpoch
    ? previousSnapshot
    : undefined;
  // A completed Native Session reconciliation is an authoritative replacement. Retaining
  // the old paging window here can resurrect rows the Agent no longer reports.
  const historyWasReconciled = previousSyncSnapshot !== undefined
    && snapshot.history_sync.state === "updated"
    && (
      snapshot.history_sync.generation > previousSyncSnapshot.history_sync.generation
      || (
        snapshot.history_sync.generation === previousSyncSnapshot.history_sync.generation
        && (
          previousSyncSnapshot.history_sync.state === "syncing"
        )
      )
    );
  const historyWasReconciledByReplacementReplica = previousReplicaEpoch !== undefined
    && previousReplicaEpoch !== replicaEpoch
    && snapshot.history_sync.state === "updated";
  if (historyWasReconciled || historyWasReconciledByReplacementReplica) return undefined;
  return retainSnapshotWindow(state.chatPages[taskId], previousSnapshot.chat, snapshot.chat);
}

function activeRequestIds(snapshot: TaskSnapshot, kind: "permission" | "elicitation") {
  return new Set(
    snapshot.active_requests.flatMap((item) => {
      const message = item.message;
      if (message.kind !== kind) return [];
      return [message.app_server_request_id, message.request_id].filter((id): id is string => Boolean(id));
    }),
  );
}

function omitKeys<T>(record: Record<string, T>, keys: Set<string>) {
  if (!keys.size) return record;
  let changed = false;
  const next: Record<string, T> = {};
  for (const [key, value] of Object.entries(record)) {
    if (keys.has(key)) {
      changed = true;
      continue;
    }
    next[key] = value;
  }
  return changed ? next : record;
}

function retainKeys<T>(record: Record<string, T>, keys: Set<string>) {
  const next = Object.fromEntries(Object.entries(record).filter(([key]) => keys.has(key)));
  return Object.keys(next).length === Object.keys(record).length ? record : next;
}

function shouldIgnoreStaleTaskSnapshot(current: TaskSnapshot | undefined, incoming: TaskSnapshot) {
  if (!current || current.task.task_id !== incoming.task.task_id) return false;
  if (incoming.revision < current.revision) return true;
  if (incoming.revision > current.revision) return false;
  if (incoming.task.message_history_version < current.task.message_history_version) return true;
  if (incoming.chat.version < current.chat.version) return true;
  if (current.task.has_messages && !incoming.task.has_messages) return true;
  if (current.chat.items.length > incoming.chat.items.length) return true;
  return false;
}

/** Merges process-local reconciliation independently from durable Task revision. */
export function reconcileTaskSnapshot(
  current: TaskSnapshot | undefined,
  incoming: TaskSnapshot,
): TaskSnapshot {
  if (!current || current.task.task_id !== incoming.task.task_id) return incoming;
  const currentSync = current.history_sync;
  const incomingSync = incoming.history_sync;
  const keepCurrent = currentSync.generation > incomingSync.generation
    || (
      currentSync.generation === incomingSync.generation
      && historySyncIsTerminal(currentSync)
      && historySyncIsPending(incomingSync)
    );
  const historySync = keepCurrent ? currentSync : incomingSync;
  const durableSnapshot = shouldIgnoreStaleTaskSnapshot(current, incoming) ? current : incoming;
  // Request responses and state events are independent transports. Preserve the
  // newer sync clock while still accepting unrelated durable snapshot growth.
  return durableSnapshot.history_sync === historySync
    ? durableSnapshot
    : { ...durableSnapshot, history_sync: historySync };
}

function historySyncIsTerminal(sync: TaskSnapshot["history_sync"]) {
  return sync.state === "idle" || sync.state === "updated";
}

function historySyncIsPending(sync: TaskSnapshot["history_sync"]) {
  return sync.state === "syncing";
}

export function upsertTaskSummary(tasks: TaskSummary[], task: TaskSummary) {
  const index = tasks.findIndex((item) => item.task_id === task.task_id);
  if (index === -1) return [task, ...tasks];
  if (sameTaskNavigationSummary(tasks[index], task)) return tasks;
  return [
    ...tasks.slice(0, index),
    task,
    ...tasks.slice(index + 1),
  ];
}

function sameTaskNavigationSummary(left: TaskSummary, right: TaskSummary) {
  // Snapshot revision clocks belong to the focused Task replica; rebuilding
  // navigation for those chat-only clocks needlessly re-sorts every sidebar row.
  return left.task_id === right.task_id
    && left.project_id === right.project_id
    && left.project_label === right.project_label
    && left.title === right.title
    && left.status === right.status
    && left.has_messages === right.has_messages
    && left.unread === right.unread
    && left.created_at === right.created_at
    && left.updated_at === right.updated_at
    && left.last_activity === right.last_activity
    && left.agent_id === right.agent_id
    && left.agent_name === right.agent_name
    && left.isolation === right.isolation
    && left.workspace_root === right.workspace_root;
}

function replaceTaskSummary(tasks: TaskSummary[], task: TaskSummary): TaskSummary[];
function replaceTaskSummary(
  tasks: TaskSummary[] | undefined,
  task: TaskSummary,
): TaskSummary[] | undefined;
function replaceTaskSummary(
  tasks: TaskSummary[] | undefined,
  task: TaskSummary,
): TaskSummary[] | undefined {
  if (!tasks) return undefined;
  const index = tasks.findIndex((item) => item.task_id === task.task_id);
  if (index === -1) return tasks;
  return [
    ...tasks.slice(0, index),
    task,
    ...tasks.slice(index + 1),
  ];
}
