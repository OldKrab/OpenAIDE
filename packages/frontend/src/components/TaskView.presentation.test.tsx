import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatMessage, TaskSnapshot } from "@openaide/app-shell-contracts";

describe("TaskView timeline presentation", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    vi.stubGlobal("window", { acquireVsCodeApi: undefined });
  });

  afterEach(() => vi.unstubAllGlobals());

  it("renders authoritative streaming text and later timeline rows without presentation gating", async () => {
    const { TaskView } = await import("./TaskView");
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<TaskView {...taskViewProps(snapshotWithAuthoritativeTail(true))} />);
    });

    const renderedTask = JSON.stringify(tree.toJSON());
    expect(renderedTask).toContain("Earlier response");
    expect(renderedTask).toContain("Steered follow-up");
    expect(renderedTask).toContain("npm test");
    expect(renderedTask).toContain("Latest update");
    expect(renderedTask).not.toContain("chat-streaming-caret");
  });
});

function taskViewProps(snapshot: TaskSnapshot) {
  return {
    appServerPermissionRequests: {},
    backendReady: true,
    chatPageState: undefined,
    dispatch: vi.fn(),
    onCancel: vi.fn(),
    onLoadChatPage: vi.fn(),
    onLoadToolDetail: vi.fn(),
    onPermissionRespond: vi.fn(),
    onRevealAttachment: vi.fn(),
    onRemoveAttachment: vi.fn(),
    onSendPrompt: vi.fn(),
    onSelectConfigOption: vi.fn(),
    permissionResponses: {},
    snapshot,
    taskInput: { prompt: "", context: [] },
    toolDetails: {},
    submitShortcut: "mod_enter" as const,
  };
}

function snapshotWithAuthoritativeTail(includeTail: boolean): TaskSnapshot {
  const revision = includeTail ? 4 : 1;
  const items: ChatMessage[] = [agentText("agent-streaming", "Earlier response", true)];
  if (includeTail) {
    items.push(
      {
        cursor: "user-steer",
        identity: "user-steer",
        message_id: "user-steer",
        message_type: "user",
        message: {
          kind: "user",
          id: "user-steer",
          text: "Steered follow-up",
          created_at: "2026-07-12T00:00:01Z",
        },
      },
      {
        cursor: "tool-running",
        identity: "tool-running",
        message_id: "tool-running",
        message_type: "activity",
        message: {
          kind: "activity",
          id: "tool-running",
          title: "Run tests",
          status: "running",
          created_at: "2026-07-12T00:00:02Z",
          collapsed: true,
          steps: [{ kind: "tool", name: "execute", status: "running", input_summary: "npm test" }],
        },
      },
      agentText("agent-later", "Latest update", false),
    );
  }

  return {
    task: {
      task_id: "task-1",
      title: "Task",
      status: "active",
      task_version: revision,
      message_history_version: revision,
      has_messages: true,
      unread: false,
      created_at: "2026-07-12T00:00:00Z",
      updated_at: "2026-07-12T00:00:03Z",
      last_activity: "2026-07-12T00:00:03Z",
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
      has_messages: true,
      total_count: items.length,
      version: revision,
    },
    permissions: [],
    send_capability: { state: "ready", attachment_only: true },
    settings_summary: { agent_id: "codex", isolation: "local" },
    revision,
  };
}

function agentText(messageId: string, text: string, streaming: boolean): ChatMessage {
  return {
    cursor: messageId,
    identity: messageId,
    message_id: messageId,
    message_type: "agent_text",
    message: {
      kind: "agent_text",
      id: messageId,
      text,
      created_at: "2026-07-12T00:00:00Z",
      streaming,
    },
  };
}
