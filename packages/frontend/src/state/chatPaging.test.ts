import { describe, expect, it } from "vitest";
import type { ActivityStep, ChatMessage, MessagePage, TaskSnapshot } from "@openaide/app-shell-contracts";
import { mergePageState, renderedChat } from "./chatPaging";

describe("chatPaging", () => {
  it("merges older page state by first-seen message id order", () => {
    const state = mergePageState(
      { olderItems: [agentMessage("m2", "old m2"), agentMessage("m3", "old m3")], hasBefore: true, pending: true, error: "loading" },
      page([agentMessage("m1", "new m1"), agentMessage("m2", "new m2")], false),
    );

    expect(state.olderItems.map((item) => item.message_id)).toEqual(["m1", "m2", "m3"]);
    expect((state.olderItems[1].message as Extract<ChatMessage["message"], { kind: "agent_message" }>).parts[0]).toEqual({
      kind: "text",
      text: "new m2",
    });
    expect(state.hasBefore).toBe(false);
    expect(state.startCursor).toBe("cursor_m1");
    expect(state.pending).toBe(false);
    expect(state.error).toBeUndefined();
  });

  it("uses page state metadata over snapshot metadata when rendering", () => {
    const chat = renderedChat(snapshot([agentMessage("m2", "tail")]), {
      olderItems: [agentMessage("m1", "older")],
      hasBefore: true,
      startCursor: "older_cursor",
      pending: true,
      error: "Page failed",
    });

    expect(chat.items.map((item) => item.message_id)).toEqual(["m1", "m2"]);
    expect(chat.hasBefore).toBe(true);
    expect(chat.beforeCursor).toBe("older_cursor");
    expect(chat.pending).toBe(true);
    expect(chat.error).toBe("Page failed");
  });

  it("preserves distinct thought identities instead of guessing chunk boundaries from adjacency", () => {
    const chat = renderedChat(snapshot([thoughtMessage("m1", "Think"), thoughtMessage("m2", "ing")]), undefined);

    expect(chat.items.map((item) => item.message_id)).toEqual(["m1", "m2"]);
    expect(chat.items.map((item) => item.message)).toMatchObject([
      { kind: "agent_message", id: "m1", role: "thought", parts: [{ kind: "text", text: "Think" }] },
      { kind: "agent_message", id: "m2", role: "thought", parts: [{ kind: "text", text: "ing" }] },
    ]);
  });

  it("preserves distinct Agent message identities instead of joining short adjacent text", () => {
    const chat = renderedChat(
      snapshot([
        agentMessage("m1", "Run"),
        agentMessage("m2", " `"),
        agentMessage("m3", "pwd"),
      ]),
      undefined,
    );

    expect(chat.items.map((item) => item.message_id)).toEqual(["m1", "m2", "m3"]);
    expect(chat.items.map((item) => item.message)).toMatchObject([
      { kind: "agent_message", id: "m1", role: "agent", parts: [{ kind: "text", text: "Run" }] },
      { kind: "agent_message", id: "m2", role: "agent", parts: [{ kind: "text", text: " `" }] },
      { kind: "agent_message", id: "m3", role: "agent", parts: [{ kind: "text", text: "pwd" }] },
    ]);
  });

  it("groups adjacent activity and thought rows into one tool run", () => {
    const chat = renderedChat(
      snapshot([
        activityMessage("m1", "exec_command", "completed", false, [
          { kind: "tool", tool_call_id: "tool-1", name: "execute", status: "completed", input_summary: "git status" },
        ]),
        thoughtMessage("thought-1", "Check the test result"),
        activityMessage("m2", "exec_command", "running", true, [
          { kind: "tool", tool_call_id: "tool-2", name: "execute", status: "running", input_summary: "npm test" },
        ]),
        activityMessage("m3", "exec_command", "error", true, [
          { kind: "command", command_label: "cargo test", status: "error", exit_code: 1 },
        ]),
      ]),
      undefined,
    );

    expect(chat.items).toHaveLength(1);
    expect(chat.items[0]).toMatchObject({
      message_id: "m1",
      cursor: "cursor_m3",
      message: {
        kind: "activity",
        title: "Commands",
        status: "completed",
        steps: [
          { kind: "tool", tool_call_id: "tool-1", input_summary: "git status" },
          { kind: "thought", message_id: "thought-1", text: "Check the test result" },
          { kind: "tool", tool_call_id: "tool-2", input_summary: "npm test" },
          { kind: "command", command_label: "cargo test" },
        ],
      },
    });
  });

  it("labels grouped activity by the work represented in the run", () => {
    const terminalChat = renderedChat(
      snapshot([
        activityMessage("m1", "write_stdin", "completed", true, [{ kind: "text", text: "npm", level: "info" }]),
        activityMessage("m2", "write_stdin", "completed", true, [{ kind: "text", text: " test", level: "info" }]),
      ]),
      undefined,
    );
    const toolChat = renderedChat(
      snapshot([
        activityMessage("m3", "Search files", "completed", true, [
          { kind: "tool", name: "search", status: "completed", input_summary: "alpha" },
        ]),
        activityMessage("m4", "Read file", "completed", true, [
          { kind: "tool", name: "read", status: "completed", input_summary: "src/main.ts" },
        ]),
      ]),
      undefined,
    );

    expect(terminalChat.items).toHaveLength(1);
    expect(terminalChat.items[0]?.message).toMatchObject({
      kind: "activity",
      title: "Terminal input",
      collapsed: true,
      steps: [{ kind: "text", text: "npm" }, { kind: "text", text: " test" }],
    });
    expect(toolChat.items).toHaveLength(1);
    expect(toolChat.items[0]?.message).toMatchObject({
      kind: "activity",
      title: "Tool activity",
      collapsed: true,
      steps: [
        { kind: "tool", input_summary: "alpha" },
        { kind: "tool", input_summary: "src/main.ts" },
      ],
    });
  });

  it("keeps a text-only Agent outcome outside adjacent Tool groups", () => {
    const chat = renderedChat(
      snapshot([
        activityMessage("tool-before", "Read file", "completed", true, [
          { kind: "tool", name: "read", status: "completed", input_summary: "src/main.ts" },
        ]),
        activityMessage("prompt-limit", "Agent stopped", "error", false, [
          { kind: "text", text: "The Agent reached its token limit.", level: "error" },
        ]),
        activityMessage("tool-after", "Search files", "completed", true, [
          { kind: "tool", name: "search", status: "completed", input_summary: "retry" },
        ]),
      ]),
      undefined,
    );

    expect(chat.items.map((item) => item.message_id)).toEqual([
      "tool-before",
      "prompt-limit",
      "tool-after",
    ]);
    expect(chat.items[1]?.message).toMatchObject({
      kind: "activity",
      title: "Agent stopped",
      status: "error",
      steps: [{ kind: "text", text: "The Agent reached its token limit." }],
    });
  });
});

function snapshot(items: ChatMessage[]): TaskSnapshot {
  return {
    lifecycle: "visible",
    task: {
      task_id: "task_1",
      title: "Task",
      status: "inactive",
      task_version: 1,
      message_history_version: 1,
      has_messages: items.length > 0,
      unread: false,
      created_at: "2026-05-17T00:00:00Z",
      updated_at: "2026-05-17T00:00:00Z",
      last_activity: "2026-05-17T00:00:00Z",
      agent_id: "codex",
      agent_name: "Codex",
      isolation: "local",
      workspace_root: "/workspace",
    },
    chat: page(items, false),
    active_requests: [],
    history_sync: { state: "idle", generation: 0 },
    send_capability: { state: "ready" },
    settings_summary: { agent_id: "codex", isolation: "local" },
    revision: 1,
  };
}

function page(items: ChatMessage[], hasBefore: boolean): MessagePage {
  return {
    task_id: "task_1",
    items,
    has_before: hasBefore,
    has_messages: items.length > 0,
    total_count: items.length,
    version: 1,
    start_cursor: items[0]?.cursor,
    end_cursor: items.at(-1)?.cursor,
  };
}

function agentMessage(id: string, text: string): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "agent_message",
    message_id: id,
    message: {
      kind: "agent_message",
      id,
      role: "agent",
      parts: [{ kind: "text", text }],
      created_at: "2026-05-17T00:00:00Z",
    },
  };
}

function thoughtMessage(id: string, text: string): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "agent_message",
    message_id: id,
    message: {
      kind: "agent_message",
      id,
      role: "thought",
      parts: [{ kind: "text", text }],
      created_at: "2026-05-17T00:00:00Z",
    },
  };
}

function activityMessage(
  id: string,
  title: string,
  status: "running" | "completed" | "error",
  collapsed: boolean,
  steps: ActivityStep[],
): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "activity",
    message_id: id,
    message: {
      kind: "activity",
      id,
      title,
      status,
      created_at: "2026-05-17T00:00:00Z",
      collapsed,
      steps,
    },
  };
}
