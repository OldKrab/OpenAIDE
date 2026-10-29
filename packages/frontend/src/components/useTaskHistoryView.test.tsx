import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it } from "vitest";
import type { ChatItem, MessageId, SubagentHistorySnapshot } from "@openaide/app-server-client";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";

import { useTaskHistoryView } from "./useTaskHistoryView";
import type { useSubagentSessions } from "./useSubagentSessions";

type Subagents = ReturnType<typeof useSubagentSessions>;

function search(messageId: string, status: "running" | "completed" = "completed"): ChatItem {
  return {
    messageId: messageId as MessageId,
    role: "agent",
    status: status === "running" ? "streaming" : "complete",
    parts: [{
      kind: "activity",
      title: `Search ${messageId}`,
      status,
      steps: [{ kind: "tool", name: "search", status, permissionOutcomes: [] }],
    }],
  } as ChatItem;
}

function reply(messageId: string, text: string): ChatItem {
  return {
    messageId: messageId as MessageId,
    role: "agent",
    status: "complete",
    parts: [{ kind: "text", text }],
  } as ChatItem;
}

function history(items: ChatItem[], revision: number): SubagentHistorySnapshot {
  return {
    taskId: "task-1",
    subagentId: "subagent-1",
    revision,
    availability: "available",
    chat: { items, hasMoreBefore: false, hasMessages: items.length > 0 },
  } as unknown as SubagentHistorySnapshot;
}

describe("useTaskHistoryView", () => {
  let latest: ReturnType<typeof useTaskHistoryView>;
  const snapshot = {
    task: { updated_at: "2026-10-28T12:00:00.000Z" },
    chat: { items: [], has_before: false },
  } as unknown as TaskSnapshot;

  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  function Probe({ subagents }: { subagents: Subagents }) {
    latest = useTaskHistoryView({ chatPageState: undefined, snapshot, subagents });
    return null;
  }

  function subagentsWith(value: SubagentHistorySnapshot) {
    return { selected: { subagentId: "subagent-1" }, history: value } as unknown as Subagents;
  }

  it("groups a Subagent's uninterrupted activity the way Main Agent Chat does", () => {
    const items = [search("s1"), search("s2"), reply("r1", "Found it."), search("s3")];
    let renderer: ReturnType<typeof create> | undefined;
    act(() => {
      renderer = create(<Probe subagents={subagentsWith(history(items, 1))} />);
    });

    expect(latest.chat.items.map((item) => item.message.kind)).toEqual(["activity", "agent_message", "activity"]);
    const group = latest.chat.items[0].message;
    expect(group.kind === "activity" ? group.steps : []).toHaveLength(2);
    act(() => renderer?.unmount());
  });

  it("keeps a settled group's identity when a later history update appends a row", () => {
    const first = [search("s1"), search("s2"), reply("r1", "Found it.")];
    let renderer: ReturnType<typeof create> | undefined;
    act(() => {
      renderer = create(<Probe subagents={subagentsWith(history(first, 1))} />);
    });
    const settled = latest.chat.items[0];

    // Each update carries fresh objects for every item, as the App Server sends them.
    const next = [...structuredClone(first), search("s3", "running")];
    act(() => renderer?.update(<Probe subagents={subagentsWith(history(next, 2))} />));

    expect(latest.chat.items).toHaveLength(3);
    expect(latest.chat.items[0]).toBe(settled);
    act(() => renderer?.unmount());
  });
});
