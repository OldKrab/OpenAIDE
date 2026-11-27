import type {
  AppServerEventPayload,
  ChatSnapshot,
  SubscriptionSnapshot,
  TaskChatChange,
  TerminalOutputSnapshot,
  ToolDetailSnapshot,
} from "./generated/protocol.js";
import type {
  EntityReplicaSnapshot,
  SubagentHistoryReplicaSnapshot,
  ToolDetailReplicaSnapshot,
} from "./backendConnection.js";
import { appendTextToMessageParts } from "./stateIngestionTask.js";
import type { SnapshotUpdate } from "./stateIngestionTypes.js";
import { changed, unchanged } from "./stateIngestionTypes.js";

/**
 * The content version of an entity copy, offered as `knownVersion` when the client subscribes
 * again (ADR-0086): a Task's Chat version, a Subagent History's revision, a Tool detail's
 * artifact sequence.
 */
export function entityVersion(snapshot: EntityReplicaSnapshot): number {
  switch (snapshot.kind) {
    case "task":
      return snapshot.chatVersion;
    case "toolDetail":
      return snapshot.details.revision;
    case "subagentHistory":
      return snapshot.history.revision;
  }
}

/**
 * Installs a Tool detail subscribe reply. The reply's fields are always current and complete.
 * Its terminal output is all of it (`full`), or that of `retained`, the copy the client offered
 * as its known version, with the missed appends added. `undefined` means the reply cannot be
 * built on `retained`, and the caller must subscribe again without a known version.
 */
export function installToolDetailReply(
  reply: Extract<SubscriptionSnapshot, { kind: "toolDetail" }>,
  retained?: ToolDetailReplicaSnapshot,
): ToolDetailReplicaSnapshot | undefined {
  const { taskId, artifactId, details, terminalSync } = reply;
  if (terminalSync.kind === "full") return { kind: "toolDetail", taskId, artifactId, details };
  if (!retained || retained.taskId !== taskId || retained.artifactId !== artifactId) return undefined;
  const terminalOutputs = terminalSync.kind === "deltas"
    ? appendTerminalOutput(retained.details.terminalOutputs ?? [], terminalSync.appends)
    : retained.details.terminalOutputs;
  return { kind: "toolDetail", taskId, artifactId, details: { ...details, terminalOutputs } };
}

/**
 * Installs a Subagent History subscribe reply: its fields are always current and complete, its
 * Chat is the tail page (`full`) or that of `retained` (`current`). `undefined` means the reply
 * cannot be built on `retained`, and the caller must subscribe again without a known version.
 */
export function installSubagentHistoryReply(
  reply: Extract<SubscriptionSnapshot, { kind: "subagentHistory" }>,
  retained?: SubagentHistoryReplicaSnapshot,
): SubagentHistoryReplicaSnapshot | undefined {
  const { history, chatSync } = reply;
  if (chatSync.kind === "full") return { kind: "subagentHistory", history };
  if (
    !retained
    || retained.history.taskId !== history.taskId
    || retained.history.subagentId !== history.subagentId
  ) return undefined;
  return {
    kind: "subagentHistory",
    history: { ...history, chat: retained.history.chat, startCursor: retained.history.startCursor },
  };
}

/** Applies one Tool detail notification: its fields and terminal appends, to its version. */
export function updateToolDetailSnapshot(
  snapshot: ToolDetailReplicaSnapshot,
  payload: AppServerEventPayload,
): SnapshotUpdate {
  if (payload.kind !== "toolDetailChanged"
    || payload.taskId !== snapshot.taskId
    || payload.artifactId !== snapshot.artifactId) {
    return unchanged(snapshot);
  }
  // A baseline can race with dispatch of the durable delta it already contains.
  if (payload.revision <= snapshot.details.revision) return unchanged(snapshot);
  if (payload.revision !== snapshot.details.revision + 1) {
    return { kind: "resyncRequired", reason: "toolDetailRevisionGap" };
  }
  let details: ToolDetailSnapshot = snapshot.details;
  let terminalOutputs = snapshot.details.terminalOutputs ?? [];
  for (const delta of payload.deltas) {
    if (delta.kind === "replaceDetails") {
      details = delta.details;
    } else {
      terminalOutputs = appendTerminalOutput(terminalOutputs, [delta]);
    }
  }
  return changed({
    ...snapshot,
    details: { ...details, revision: payload.revision, terminalOutputs },
  });
}

/**
 * Applies one Subagent History revision (ADR-0046). A revision that does not continue the held
 * copy, or a change to a message the copy should hold but does not, makes the replica re-read
 * the history.
 */
export function updateSubagentHistorySnapshot(
  snapshot: SubagentHistoryReplicaSnapshot,
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

function appendTerminalOutput(
  outputs: TerminalOutputSnapshot[],
  appends: { terminalId: string; data: string }[],
): TerminalOutputSnapshot[] {
  const next = outputs.map((terminal) => ({ ...terminal }));
  for (const append of appends) {
    const existing = next.find((terminal) => terminal.terminalId === append.terminalId);
    if (existing) existing.output += append.data;
    else next.push({ terminalId: append.terminalId, output: append.data });
  }
  return next;
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
