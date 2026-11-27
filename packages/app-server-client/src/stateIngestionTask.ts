import type {
  AppServerEventPayload,
  ChatItem,
  MessagePart,
  TaskChatChange,
  SubscriptionSnapshot,
  TaskSnapshot,
} from "./generated/protocol.js";
import type { TaskReplicaSnapshot } from "./backendConnection.js";
import type { SnapshotUpdate } from "./stateIngestionTypes.js";
import { changed, unchanged } from "./stateIngestionTypes.js";

export function updateTaskSnapshot(
  snapshot: TaskReplicaSnapshot,
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
    default:
      return unchanged(snapshot);
  }
}

/**
 * Installs a Task subscribe reply (ADR-0086). The reply's fields are always current and
 * complete. Its Chat is the tail page (`full`), or the Chat of `retained`, the copy the client
 * offered as its known version, brought current with the missed `deltas`. `undefined` means
 * the reply cannot be built on `retained`, and the caller must subscribe again without a
 * known version.
 */
export function installTaskReply(
  reply: Extract<SubscriptionSnapshot, { kind: "task" }>,
  retained?: TaskReplicaSnapshot,
): TaskReplicaSnapshot | undefined {
  const { task, chatVersion, chatSync } = reply;
  if (chatSync.kind === "full") return { kind: "task", task, chatVersion };
  if (!retained || retained.task.task.taskId !== task.task.taskId) return undefined;
  let current: TaskSnapshot = { ...task, chat: retained.task.chat };
  if (chatSync.kind === "deltas") {
    for (const change of chatSync.changes) {
      const result = applyChatChange(current, change);
      if (result.kind === "resyncRequired") return undefined;
      current = result.task;
    }
  }
  return { kind: "task", task: current, chatVersion };
}

function applyTaskChanges(
  snapshot: TaskReplicaSnapshot,
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
    backgroundCommands: changes.backgroundCommands ?? snapshot.task.backgroundCommands,
    lifecycle: changes.lifecycle ?? snapshot.task.lifecycle,
    preparation: changes.preparation ?? snapshot.task.preparation,
    permissionPolicy: changes.permissionPolicy ?? snapshot.task.permissionPolicy,
    agentConfig: changes.agentConfig ?? snapshot.task.agentConfig,
    agentCommands: changes.agentCommands ?? snapshot.task.agentCommands,
    sendCapability: changes.sendCapability ?? snapshot.task.sendCapability,
    messageQueue: changes.messageQueue ?? snapshot.task.messageQueue,
    inputCapabilities: changes.inputCapabilities ?? snapshot.task.inputCapabilities,
    contextUsage: changes.contextUsage === undefined
      ? snapshot.task.contextUsage
      : changes.contextUsage,
    currentPlan: changes.currentPlan === undefined
      ? snapshot.task.currentPlan
      : changes.currentPlan,
  };

  for (const chatChange of changes.chat ?? []) {
    const result = applyChatChange(task, chatChange);
    if (result.kind === "resyncRequired") return result;
    task = result.task;
  }
  return changed({ ...snapshot, task, chatVersion: changes.chatVersion ?? snapshot.chatVersion });
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

export function appendTextToMessageParts(parts: MessagePart[], text: string): MessagePart[] {
  const last = parts.at(-1);
  if (last?.kind !== "text") return [...parts, { kind: "text", text }];
  return [...parts.slice(0, -1), { ...last, text: last.text + text }];
}
