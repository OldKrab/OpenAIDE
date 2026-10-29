import type {
  AppServerEventPayload,
  ChatSnapshot,
  TaskChatChange,
} from "./generated/protocol.js";
import type { ReplicaSnapshot } from "./backendConnection.js";
import { appendTextToMessageParts } from "./stateIngestionTask.js";
import type { SnapshotUpdate } from "./stateIngestionTypes.js";
import { changed, unchanged } from "./stateIngestionTypes.js";

type SubagentHistoryReplica = Extract<ReplicaSnapshot, { kind: "subagentHistory" }>;

/**
 * Applies one Subagent History revision (ADR-0046). A revision that does not
 * continue the held copy, or a change to a message the copy should hold but
 * does not, makes the replica re-read the history.
 */
export function updateSubagentHistorySnapshot(
  snapshot: SubagentHistoryReplica,
  payload: AppServerEventPayload,
): SnapshotUpdate {
  const history = snapshot.history;
  if (payload.kind !== "subagentHistoryChanged"
    || payload.taskId !== history.taskId
    || payload.subagentId !== history.subagentId) {
    return unchanged(snapshot);
  }
  // A baseline can race with dispatch of a revision it already contains.
  if (payload.revision <= history.revision) return unchanged(snapshot);
  if (payload.revision !== history.revision + 1) {
    return { kind: "resyncRequired", reason: "subagentHistoryRevisionGap" };
  }
  let chat = history.chat;
  for (const change of payload.chat ?? []) {
    const next = applyChatChange(chat, change);
    if (!next) return { kind: "resyncRequired", reason: "missingChatItem" };
    chat = next;
  }
  return changed({
    kind: "subagentHistory",
    history: {
      ...history,
      revision: payload.revision,
      availability: chat.hasMessages ? "available" : history.availability,
      chat,
      currentPlan: payload.currentPlan === undefined ? history.currentPlan : payload.currentPlan,
    },
  });
}

/**
 * Returns the changed copy, or `undefined` when the change names a message the
 * copy should hold. A message missing from a copy with older pages unloaded
 * belongs to those pages, so its change is skipped.
 */
function applyChatChange(chat: ChatSnapshot, change: TaskChatChange): ChatSnapshot | undefined {
  switch (change.kind) {
    case "append":
      return { ...chat, items: [...chat.items, change.item], hasMessages: true };
    case "replace":
      return change.chat;
    case "upsert":
    case "appendText": {
      const messageId = change.kind === "upsert" ? change.item.messageId : change.messageId;
      const index = chat.items.findIndex((item) => item.messageId === messageId);
      if (index === -1) return chat.hasMoreBefore ? chat : undefined;
      const items = chat.items.map((item, itemIndex) => {
        if (itemIndex !== index) return item;
        return change.kind === "upsert"
          ? change.item
          : { ...item, parts: appendTextToMessageParts(item.parts, change.text) };
      });
      return { ...chat, items };
    }
  }
}
