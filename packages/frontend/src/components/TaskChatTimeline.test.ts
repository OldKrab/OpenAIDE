import { describe, expect, it } from "vitest";
import type { ChatMessage } from "@openaide/app-shell-contracts";
import { isLiveTextMessage } from "./TaskChatTimeline";

describe("TaskChatTimeline live text ownership", () => {
  it("does not treat an earlier completed Agent message as streaming", () => {
    const previous = agentMessage("agent-previous");
    const current = agentMessage("agent-current");
    const presentation = {
      agent: { eventCursor: "cursor-current", messageId: current.message_id },
    };

    expect(isLiveTextMessage(presentation, previous)).toBe(false);
    expect(isLiveTextMessage(presentation, current)).toBe(true);
  });
});

function agentMessage(messageId: string): ChatMessage {
  return {
    cursor: `cursor-${messageId}`,
    identity: messageId,
    message_type: "agent_message",
    message_id: messageId,
    message: {
      kind: "agent_message",
      id: messageId,
      created_at: "2026-08-12T00:00:00.000Z",
      role: "agent",
      parts: [],
    },
  };
}
