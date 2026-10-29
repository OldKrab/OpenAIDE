import { describe, expect, it } from "vitest";
import type {
  AppServerEventPayload,
  ChatItem,
  MessageId,
  SubagentId,
  SubagentHistorySnapshot,
  TaskId,
} from "./generated/protocol.js";
import { updateSubagentHistorySnapshot } from "./stateIngestionSubagentHistory.js";

const taskId = "task_1" as TaskId;
const subagentId = "subagent_1" as SubagentId;

function item(messageId: string, text: string): ChatItem {
  return {
    messageId: messageId as MessageId,
    role: "agent",
    status: "complete",
    parts: [{ kind: "text", text }],
  };
}

function replica(revision: number, items: ChatItem[], hasMoreBefore = false) {
  const history: SubagentHistorySnapshot = {
    taskId,
    subagentId,
    revision,
    availability: items.length > 0 ? "available" : "waitingForActivity",
    chat: { items, hasMoreBefore, hasMessages: items.length > 0 },
  };
  return { kind: "subagentHistory" as const, history };
}

function changed(
  revision: number,
  changes: Partial<Extract<AppServerEventPayload, { kind: "subagentHistoryChanged" }>>,
): AppServerEventPayload {
  return { kind: "subagentHistoryChanged", taskId, subagentId, revision, ...changes };
}

function historyAfter(update: ReturnType<typeof updateSubagentHistorySnapshot>) {
  if (update.kind !== "updated" || update.snapshot.kind !== "subagentHistory") {
    throw new Error(`expected an updated history, got ${update.kind}`);
  }
  return update.snapshot.history;
}

describe("Subagent history revisions", () => {
  it("applies consecutive revisions of messages, text and plan", () => {
    let snapshot = replica(0, []);
    const plan = { entries: [{ content: "Run tests", priority: "high", status: "pending" }] } as never;
    for (const payload of [
      changed(1, { chat: [{ kind: "append", item: item("reply", "Done") }] }),
      changed(2, { chat: [{ kind: "appendText", messageId: "reply" as MessageId, text: " now" }] }),
      changed(3, { currentPlan: plan }),
      changed(4, { chat: [{ kind: "upsert", item: item("reply", "Rewritten") }] }),
      changed(5, { currentPlan: null }),
    ]) {
      snapshot = { kind: "subagentHistory", history: historyAfter(updateSubagentHistorySnapshot(snapshot, payload)) };
      if (snapshot.history.revision === 3) {
        expect(snapshot.history.chat.items[0]?.parts).toEqual([{ kind: "text", text: "Done now" }]);
        expect(snapshot.history.currentPlan).toBe(plan);
      }
    }
    expect(snapshot.history).toMatchObject({
      revision: 5,
      availability: "available",
      currentPlan: null,
      chat: { items: [item("reply", "Rewritten")], hasMessages: true },
    });
  });

  it("ignores a revision the copy already holds", () => {
    const snapshot = replica(2, [item("reply", "Done")]);
    const update = updateSubagentHistorySnapshot(
      snapshot,
      changed(2, { chat: [{ kind: "appendText", messageId: "reply" as MessageId, text: "!" }] }),
    );
    expect(update).toEqual({ kind: "updated", snapshot, changed: false });
  });

  it("re-reads the history when a revision is skipped", () => {
    const update = updateSubagentHistorySnapshot(
      replica(2, [item("reply", "Done")]),
      changed(4, { chat: [{ kind: "append", item: item("next", "Hi") }] }),
    );
    expect(update).toEqual({ kind: "resyncRequired", reason: "subagentHistoryRevisionGap" });
  });

  it("re-reads the history when a held page lacks the changed message", () => {
    const update = updateSubagentHistorySnapshot(
      replica(1, [item("reply", "Done")]),
      changed(2, { chat: [{ kind: "appendText", messageId: "missing" as MessageId, text: "!" }] }),
    );
    expect(update).toEqual({ kind: "resyncRequired", reason: "missingChatItem" });
  });

  it("skips a change to a message in an unloaded older page", () => {
    const history = historyAfter(updateSubagentHistorySnapshot(
      replica(1, [item("reply", "Done")], true),
      changed(2, { chat: [{ kind: "upsert", item: item("older", "Updated") }] }),
    ));
    expect(history.revision).toBe(2);
    expect(history.chat.items).toEqual([item("reply", "Done")]);
  });
});
