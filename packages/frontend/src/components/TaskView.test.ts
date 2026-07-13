import { describe, expect, it } from "vitest";
import type { ChatMessage } from "@openaide/app-shell-contracts";
import { chatItemsWithResolvedPermissions } from "./taskChatPresentation";
import { scrollTopAfterPrependedContent } from "./TaskViewModel";

describe("TaskView presentation", () => {
  it("keeps resolved permission history beside its matching Tool activity", () => {
    const items = chatItemsWithResolvedPermissions([
      activity("activity-1", "tool-1"),
      agentText("agent-1"),
      permission("permission-1", "tool-1"),
    ]);

    expect(items.map((item) => item.message_id)).toEqual([
      "activity-1",
      "permission-1",
      "agent-1",
    ]);
  });

  it("keeps unmatched permission history in App Server order at the tail", () => {
    const items = chatItemsWithResolvedPermissions([
      agentText("agent-1"),
      permission("permission-1", "tool-missing"),
    ]);

    expect(items.map((item) => item.message_id)).toEqual(["agent-1", "permission-1"]);
  });

  it("keeps the same visible content anchored after earlier messages prepend", () => {
    expect(scrollTopAfterPrependedContent({
      previousScrollHeight: 1000,
      previousScrollTop: 240,
      nextScrollHeight: 1380,
    })).toBe(620);
  });
});

function activity(id: string, toolCallId: string): ChatMessage {
  return {
    cursor: id,
    identity: id,
    message_id: id,
    message_type: "activity",
    message: {
      kind: "activity",
      id,
      title: "Run command",
      status: "completed",
      created_at: "2026-07-13T00:00:00Z",
      collapsed: true,
      steps: [{ kind: "tool", tool_call_id: toolCallId, name: "execute", status: "completed" }],
    },
  };
}

function permission(id: string, toolCallId: string): ChatMessage {
  return {
    cursor: id,
    identity: id,
    message_id: id,
    message_type: "permission",
    message: {
      kind: "permission",
      id,
      request_id: `agent-${id}`,
      title: "Run command",
      tool_call: { id: toolCallId, title: "Run command", kind: "execute" },
      state: "resolved",
      created_at: "2026-07-13T00:00:01Z",
      options: [{ id: "allow", label: "Allow", kind: "allow" }],
      selected_option: "allow",
      decision: "approved",
    },
  };
}

function agentText(id: string): ChatMessage {
  return {
    cursor: id,
    identity: id,
    message_id: id,
    message_type: "agent_text",
    message: { kind: "agent_text", id, text: "Done", created_at: "2026-07-13T00:00:02Z" },
  };
}
