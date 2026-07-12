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
    expect((state.olderItems[1].message as Extract<ChatMessage["message"], { kind: "agent_text" }>).text).toBe("new m2");
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

  it("coalesces streamed thought rows using first identity, last cursor, text order, and streaming flag", () => {
    const chat = renderedChat(snapshot([thoughtMessage("m1", "Think", false), thoughtMessage("m2", "ing", true)]), undefined);

    expect(chat.items).toHaveLength(1);
    expect(chat.items[0]).toMatchObject({
      identity: "m1",
      message_id: "m1",
      cursor: "cursor_m2",
      message: { kind: "thought", id: "m1", text: "Thinking", streaming: true },
    });
  });

  it("coalesces streamed agent text chunks using first identity, last cursor, text order, and streaming flag", () => {
    const chat = renderedChat(
      snapshot([
        agentMessage("m1", "Run", false),
        agentMessage("m2", " `", true),
        agentMessage("m3", "pwd", false),
      ]),
      undefined,
    );

    expect(chat.items).toHaveLength(1);
    expect(chat.items[0]).toMatchObject({
      identity: "m1",
      message_id: "m1",
      cursor: "cursor_m3",
      message: { kind: "agent_text", id: "m1", text: "Run `pwd", streaming: true },
    });
  });

  it("coalesces activity runs without promoting a failed tool to the group status", () => {
    const chat = renderedChat(
      snapshot([
        activityMessage("m1", "exec_command", "completed", false, [
          { kind: "tool", name: "execute", status: "completed", input_summary: "git status" },
        ]),
        activityMessage("m2", "exec_command", "running", true, [
          { kind: "tool", name: "execute", status: "running", input_summary: "npm test" },
        ]),
        activityMessage("m3", "exec_command", "error", true, [
          { kind: "command", command_label: "cargo test", status: "error", exit_code: 1 },
        ]),
      ]),
      undefined,
    );

    expect(chat.items).toHaveLength(1);
    expect(chat.items[0]).toMatchObject({
      identity: "m1",
      message_id: "m1",
      cursor: "cursor_m3",
      message: {
        kind: "activity",
        title: "Commands",
        status: "completed",
        collapsed: false,
        steps: [
          { kind: "tool", input_summary: "git status" },
          { kind: "tool", input_summary: "npm test" },
          { kind: "command", command_label: "cargo test" },
        ],
      },
    });
  });

  it("classifies coalesced terminal input and generic tool activity runs", () => {
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

    expect(terminalChat.items[0]).toMatchObject({
      cursor: "cursor_m2",
      message: { kind: "activity", title: "Terminal input", collapsed: true },
    });
    expect(toolChat.items[0]).toMatchObject({
      cursor: "cursor_m4",
      message: { kind: "activity", title: "Tool activity", collapsed: true },
    });
  });
});

function snapshot(items: ChatMessage[]): TaskSnapshot {
  return {
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
    permissions: [],
    history_sync: { state: "idle", generation: 0 },
    send_capability: { state: "ready", attachment_only: true },
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

function agentMessage(id: string, text: string, streaming = false): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "agent_text",
    message_id: id,
    message: {
      kind: "agent_text",
      id,
      text,
      created_at: "2026-05-17T00:00:00Z",
      streaming,
    },
  };
}

function thoughtMessage(id: string, text: string, streaming = false): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "thought",
    message_id: id,
    message: {
      kind: "thought",
      id,
      text,
      created_at: "2026-05-17T00:00:00Z",
      streaming,
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
