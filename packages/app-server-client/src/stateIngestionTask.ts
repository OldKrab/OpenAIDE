import type {
  AppServerEventPayload,
  ChatItem,
  MessagePart,
  PendingRequestSnapshot,
  SubscriptionSnapshot,
  TaskChatChange,
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
    case "taskChanged":
      if (payload.taskId !== task.task.taskId || payload.revision <= task.revision) return unchanged(snapshot);
      if (payload.revision !== task.revision + 1) return { kind: "resyncRequired", reason: "taskRevisionGap" };
      return applyTaskChanges(snapshot, payload.revision, payload.changes);
    case "taskHistorySyncUpdated":
      return payload.taskId === task.task.taskId
        && payload.historySync.generation >= task.historySync.generation
        ? changed({ ...snapshot, task: { ...task, historySync: payload.historySync } })
        : unchanged(snapshot);
    case "taskRequestsUpdated":
      return payload.taskId === task.task.taskId
        ? changed({ ...snapshot, task: { ...task, pendingRequests: payload.requests } })
        : unchanged(snapshot);
    case "requestUpdated":
      return requestMatchesTask(payload.request, task.task.taskId)
        ? changed({ ...snapshot, task: upsertPendingRequest(task, payload.request) })
        : unchanged(snapshot);
    default:
      return unchanged(snapshot);
  }
}

function applyTaskChanges(
  snapshot: Extract<SubscriptionSnapshot, { kind: "task" }>,
  revision: number,
  changes: Extract<AppServerEventPayload, { kind: "taskChanged" }>["changes"],
): SnapshotUpdate {
  let task: TaskSnapshot = {
    ...snapshot.task,
    revision,
    task: changes.task ?? snapshot.task.task,
    activeTurnStartedAt: changes.activeTurnStartedAt === undefined
      ? snapshot.task.activeTurnStartedAt
      : changes.activeTurnStartedAt,
    lifecycle: changes.lifecycle ?? snapshot.task.lifecycle,
    preparation: changes.preparation ?? snapshot.task.preparation,
    agentConfig: changes.agentConfig ?? snapshot.task.agentConfig,
    agentCommands: changes.agentCommands ?? snapshot.task.agentCommands,
    sendCapability: changes.sendCapability ?? snapshot.task.sendCapability,
    inputCapabilities: changes.inputCapabilities ?? snapshot.task.inputCapabilities,
  };

  for (const chatChange of changes.chat ?? []) {
    const result = applyChatChange(task, chatChange);
    if (result.kind === "resyncRequired") return result;
    task = result.task;
  }
  return changed({ ...snapshot, task });
}

type ChatApplyResult = { kind: "applied"; task: TaskSnapshot }
  | { kind: "resyncRequired"; reason: "missingChatItem" };

function applyChatChange(task: TaskSnapshot, change: TaskChatChange): ChatApplyResult {
  switch (change.kind) {
    case "append":
      return { kind: "applied", task: appendChatItem(task, change.item) };
    case "upsert":
      return { kind: "applied", task: upsertChatItem(task, change.item) };
    case "appendText":
      return updateChatItemText(task, change.messageId, change.text);
    case "replace":
      return { kind: "applied", task: { ...task, chat: change.chat } };
  }
}

function appendChatItem(task: TaskSnapshot, item: ChatItem): TaskSnapshot {
  return {
    ...task,
    chat: {
      ...task.chat,
      items: [...task.chat.items, item],
      hasMessages: true,
    },
  };
}

function upsertChatItem(task: TaskSnapshot, item: ChatItem): TaskSnapshot {
  const existing = task.chat.items.findIndex((candidate) => candidate.messageId === item.messageId);
  const items = existing === -1
    ? [...task.chat.items, item]
    : task.chat.items.map((candidate, index) => index === existing ? item : candidate);
  return {
    ...task,
    chat: {
      ...task.chat,
      items,
      hasMessages: true,
    },
  };
}

function updateChatItemText(task: TaskSnapshot, messageId: string, text: string): ChatApplyResult {
  const itemIndex = task.chat.items.findIndex((item) => item.messageId === messageId);
  if (itemIndex === -1) return { kind: "resyncRequired", reason: "missingChatItem" };

  const items = task.chat.items.map((item, index) => {
    if (index !== itemIndex) return item;
    return { ...item, parts: appendTextToMessageParts(item.parts, text) };
  });
  return {
    kind: "applied",
    task: {
      ...task,
      chat: {
        ...task.chat,
        items,
        hasMessages: task.chat.hasMessages || items.length > 0,
      },
    },
  };
}

function appendTextToMessageParts(parts: MessagePart[], text: string): MessagePart[] {
  const last = parts.at(-1);
  if (last?.kind !== "text") return [...parts, { kind: "text", text }];
  return [...parts.slice(0, -1), { ...last, text: last.text + text }];
}

function upsertPendingRequest(task: TaskSnapshot, request: PendingRequestSnapshot): TaskSnapshot {
  const pendingRequests = task.pendingRequests ?? [];
  const existing = pendingRequests.findIndex((pending) => pending.requestId === request.requestId);
  const next = existing === -1
    ? [...pendingRequests, request]
    : pendingRequests.map((pending, index) => index === existing ? request : pending);
  return { ...task, pendingRequests: next };
}

function requestMatchesTask(request: PendingRequestSnapshot, taskId: TaskId): boolean {
  return request.scope.kind === "task" && request.scope.taskId === taskId;
}
