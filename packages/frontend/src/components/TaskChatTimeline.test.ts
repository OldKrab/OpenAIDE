import { describe, expect, it } from "vitest";
import type { ChatMessage } from "@openaide/app-shell-contracts";
import { followsAgentAnswer, isLiveTextMessage, type TaskChatTimelineRow } from "./TaskChatTimeline";

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

describe("TaskChatTimeline message separation", () => {
  it("separates an Agent answer only when it directly follows another Agent answer", () => {
    const answer = messageRow(agentMessage("answer-1"));
    const nextAnswer = messageRow(agentMessage("answer-2"));
    const thought = messageRow(agentMessage("thought-1", "thought"));

    expect(followsAgentAnswer(answer, nextAnswer)).toBe(true);
    expect(followsAgentAnswer(thought, nextAnswer)).toBe(false);
    expect(followsAgentAnswer(answer, thought)).toBe(false);
    expect(followsAgentAnswer(undefined, nextAnswer)).toBe(false);
    expect(followsAgentAnswer(answer, { key: "timeline-status", kind: "status" })).toBe(false);
  });
});

function messageRow(message: ChatMessage): TaskChatTimelineRow {
  return { key: message.message_id, kind: "message", message };
}

function agentMessage(messageId: string, role: "agent" | "thought" = "agent"): ChatMessage {
  return {
    cursor: `cursor-${messageId}`,
    identity: messageId,
    message_type: "agent_message",
    message_id: messageId,
    message: {
      kind: "agent_message",
      id: messageId,
      created_at: "2026-08-12T00:00:00.000Z",
      role,
      parts: [],
    },
  };
}
