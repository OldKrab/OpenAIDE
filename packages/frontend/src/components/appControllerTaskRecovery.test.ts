import { describe, expect, it } from "vitest";
import type { ChatMessage, TaskSnapshot } from "@openaide/app-shell-contracts";
import { preparedPromptNeedsRetry } from "./appControllerTaskRecovery";

describe("preparedPromptNeedsRetry", () => {
  it("retries only the preserved send intent after preparation clears", () => {
    expect(
      preparedPromptNeedsRetry(snapshot("inactive"), {
        prompt: "hi",
        context: [],
        error: "Task Agent preparation is still running",
      }),
    ).toBe(true);
    expect(
      preparedPromptNeedsRetry(snapshot("blocked"), {
        prompt: "hi",
        context: [],
        error: "Task Agent preparation is still running",
      }),
    ).toBe(false);
    expect(
      preparedPromptNeedsRetry(snapshot("inactive"), {
        prompt: "hi",
        context: [],
        error: "Different error",
      }),
    ).toBe(false);
  });
});

function snapshot(status: TaskSnapshot["task"]["status"], items: ChatMessage[] = []): TaskSnapshot {
  return {
    task: {
      task_id: "task-1",
      title: "Task",
      status,
      task_version: 1,
      message_history_version: 1,
      has_messages: items.length > 0,
      unread: false,
      created_at: "2026-06-28T00:00:00Z",
      updated_at: "2026-06-28T00:00:00Z",
      last_activity: "2026-06-28T00:00:00Z",
      agent_id: "codex",
      agent_name: "Codex",
      isolation: "local",
      workspace_root: "/workspace",
    },
    history_sync: { state: "idle", generation: 0 },
    chat: {
      task_id: "task-1",
      items,
      has_before: false,
      has_messages: items.length > 0,
      total_count: items.length,
      version: 1,
    },
    permissions: [],
    send_capability: { state: "ready", attachment_only: true },
    settings_summary: { agent_id: "codex", isolation: "local" },
    revision: 1,
  };
}
