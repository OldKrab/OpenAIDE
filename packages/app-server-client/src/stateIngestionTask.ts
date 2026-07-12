import type {
  AppServerEventPayload,
  ChatItem,
  MessagePart,
  PendingRequestSnapshot,
  SubscriptionSnapshot,
  TaskId,
  TaskSnapshot,
} from "./generated/protocol.js";
import type { SnapshotUpdate } from "./stateIngestionTypes.js";
import { changed, unchanged } from "./stateIngestionTypes.js";

export function updateTaskSnapshot(
  snapshot: Extract<SubscriptionSnapshot, { kind: "task" }>,
  payload: AppServerEventPayload,
): SnapshotUpdate {
  const task = snapshot.task;

  switch (payload.kind) {
    case "taskUpdated":
      return payload.task.taskId === task.task.taskId ? changed({ ...snapshot, task: { ...task, task: payload.task } }) : unchanged(snapshot);
    case "taskSnapshotUpdated":
      return payload.task.task.taskId === task.task.taskId
        ? changed({ ...snapshot, task: payload.task })
        : unchanged(snapshot);
    case "taskHistorySyncUpdated":
      return payload.taskId === task.task.taskId
        && payload.historySync.generation >= task.historySync.generation
        ? changed({ ...snapshot, task: { ...task, historySync: payload.historySync } })
        : unchanged(snapshot);
    case "chatItemAppended":
      return payload.taskId === task.task.taskId && payload.revision > task.revision
        ? changed({ ...snapshot, task: appendChatItem(task, payload.revision, payload.item) })
        : unchanged(snapshot);
    case "chatItemChunk":
      if (payload.taskId !== task.task.taskId) return unchanged(snapshot);
      if (payload.revision <= task.revision) return unchanged(snapshot);
      return updateChatItemChunk(task, payload.revision, payload.messageId, payload.chunk.text, payload.chunk.finalChunk === true);
    case "requestUpdated":
      return requestMatchesTask(payload.request, task.task.taskId)
        ? changed({ ...snapshot, task: upsertPendingRequest(task, payload.request) })
        : unchanged(snapshot);
    default:
      return unchanged(snapshot);
  }
}

function appendChatItem(task: TaskSnapshot, revision: number, item: ChatItem): TaskSnapshot {
  return {
    ...task,
    revision,
    chat: {
      ...task.chat,
      items: [...task.chat.items, item],
      hasMessages: true,
    },
  };
}

function updateChatItemChunk(task: TaskSnapshot, revision: number, messageId: string, text: string, finalChunk: boolean): SnapshotUpdate {
  const itemIndex = task.chat.items.findIndex((item) => item.messageId === messageId);
  if (itemIndex === -1) return { kind: "resyncRequired", reason: "missingChatItem" };

  const items = task.chat.items.map((item, index) => {
    if (index !== itemIndex) return item;
    return {
      ...item,
      status: finalChunk ? ("complete" as const) : item.status,
      parts: appendTextToMessageParts(item.parts, text),
    };
  });

  return changed({
    kind: "task",
    task: {
      ...task,
      revision,
      chat: {
        ...task.chat,
        items,
        hasMessages: task.chat.hasMessages || items.length > 0,
      },
    },
  });
}

function appendTextToMessageParts(parts: MessagePart[], text: string): MessagePart[] {
  const last = parts.at(-1);
  if (last?.kind !== "text") return [...parts, { kind: "text", text }];

  return [...parts.slice(0, -1), { ...last, text: last.text + text }];
}

function upsertPendingRequest(task: TaskSnapshot, request: PendingRequestSnapshot): TaskSnapshot {
  const pendingRequests = task.pendingRequests ?? [];
  const existing = pendingRequests.findIndex((pending) => pending.requestId === request.requestId);
  const next =
    existing === -1
      ? [...pendingRequests, request]
      : pendingRequests.map((pending, index) => (index === existing ? request : pending));

  return { ...task, pendingRequests: next };
}

function requestMatchesTask(request: PendingRequestSnapshot, taskId: TaskId): boolean {
  return request.scope.kind === "task" && request.scope.taskId === taskId;
}
