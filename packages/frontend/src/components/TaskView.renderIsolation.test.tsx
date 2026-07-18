import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatMessage, TaskSnapshot } from "@openaide/app-shell-contracts";

const chatRowRender = vi.hoisted(() => vi.fn());

vi.mock("./ChatMessageView", () => ({
  ChatRow: ({ message }: { message: ChatMessage }) => {
    chatRowRender(message.message_id);
    return <p>{message.message_id}</p>;
  },
}));

import { TaskView } from "./TaskView";

describe("TaskView render isolation", () => {
  beforeEach(() => {
    chatRowRender.mockClear();
    vi.useFakeTimers();
    vi.stubGlobal("window", {
      acquireVsCodeApi: undefined,
      clearTimeout: globalThis.clearTimeout,
      setTimeout: globalThis.setTimeout,
    });
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("does not render unchanged Chat rows when only the composer draft changes", () => {
    const snapshot = taskSnapshot();
    const sharedState = {
      permissionResponses: {},
      questionResponses: {},
      toolDetails: {},
    };
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(
        <TaskView
          {...taskViewProps(snapshot)}
          {...sharedState}
          taskInput={{ prompt: "", context: [] }}
        />,
      );
    });

    act(() => {
      tree.update(
        <TaskView
          {...taskViewProps(snapshot)}
          {...sharedState}
          taskInput={{ prompt: "a", context: [] }}
        />,
      );
    });

    expect(chatRowRender).toHaveBeenCalledOnce();
  });

  it("keeps live text animation frames below the Chat timeline seam", async () => {
    const initial = taskSnapshot();
    initial.task.status = "active";
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<TaskView {...taskViewProps(initial)} />);
    });

    const updated = structuredClone(initial);
    const message = updated.chat.items[0]?.message;
    if (message?.kind !== "agent_message" || message.parts[0]?.kind !== "text") {
      throw new Error("expected Agent text");
    }
    message.parts[0].text = "Stable answer with a live suffix";
    act(() => {
      tree.update(
        <TaskView
          {...taskViewProps(updated)}
          liveTextPresentation={{
            agent: { messageId: "agent-1", eventCursor: "cursor-live-1" },
          }}
        />,
      );
    });
    const rendersAfterAuthoritativeUpdate = chatRowRender.mock.calls.length;

    await act(async () => {
      await vi.advanceTimersByTimeAsync(20);
    });

    expect(chatRowRender).toHaveBeenCalledTimes(rendersAfterAuthoritativeUpdate);
  });

  it("updates elapsed turn time without rendering unchanged Chat rows", async () => {
    vi.setSystemTime(new Date("2026-07-13T00:01:24Z"));
    const snapshot = taskSnapshot();
    snapshot.task.status = "active";
    snapshot.active_turn_started_at = String(new Date("2026-07-13T00:00:00Z").getTime());
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<TaskView {...taskViewProps(snapshot)} />);
    });

    expect(tree.root.findByProps({ className: "working-status-duration" }).children).toContain("1:24");
    expect(tree.root.findAllByProps({ className: "working-status-duration-separator" })).toHaveLength(1);
    const rendersBeforeTick = chatRowRender.mock.calls.length;

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });

    expect(tree.root.findByProps({ className: "working-status-duration" }).children).toContain("1:25");
    expect(chatRowRender).toHaveBeenCalledTimes(rendersBeforeTick);
  });

  it("keeps the elapsed timer quiet for the first five seconds", async () => {
    vi.setSystemTime(new Date("2026-07-13T00:00:04Z"));
    const snapshot = taskSnapshot();
    snapshot.task.status = "active";
    snapshot.active_turn_started_at = "2026-07-13T00:00:00Z";
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<TaskView {...taskViewProps(snapshot)} />);
    });
    expect(tree.root.findAllByProps({ className: "working-status-duration" })).toHaveLength(0);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });

    expect(tree.root.findByProps({ className: "working-status-duration" }).children).toContain("0:05");
  });
});

function taskViewProps(snapshot: TaskSnapshot) {
  return {
    backendReady: true,
    chatPageState: undefined,
    intents: {
      changePrompt: vi.fn(),
      recordScroll: vi.fn(),
      refreshWorkspace: vi.fn(),
      reportAttachmentError: vi.fn(),
    },
    onCancel: vi.fn(),
    onLoadChatPage: vi.fn(),
    onSubscribeToolDetail: vi.fn(() => vi.fn()),
    onPermissionRespond: vi.fn(),
    onQuestionRespond: vi.fn(),
    onRevealAttachment: vi.fn(),
    onRemoveAttachment: vi.fn(),
    onSendPrompt: vi.fn(),
    onSelectConfigOption: vi.fn(),
    permissionResponses: {},
    questionResponses: {},
    snapshot,
    taskInput: { prompt: "", context: [] },
    toolDetails: {},
    submitShortcut: "mod_enter" as const,
  };
}

function taskSnapshot(): TaskSnapshot {
  const message: ChatMessage = {
    cursor: "agent-1",
    identity: "agent-1",
    message_id: "agent-1",
    message_type: "agent_message",
    message: {
      kind: "agent_message",
      id: "agent-1",
      role: "agent",
      parts: [{ kind: "text", text: "Stable answer" }],
      created_at: "2026-07-13T00:00:00Z",
    },
  };
  return {
    lifecycle: "visible",
    task: {
      task_id: "task-1",
      title: "Task",
      status: "inactive",
      task_version: 1,
      message_history_version: 1,
      has_messages: true,
      unread: false,
      created_at: "2026-07-13T00:00:00Z",
      updated_at: "2026-07-13T00:00:00Z",
      last_activity: "2026-07-13T00:00:00Z",
      agent_id: "codex",
      agent_name: "Codex",
      isolation: "local",
      workspace_root: "/workspace",
    },
    history_sync: { state: "idle", generation: 0 },
    chat: {
      task_id: "task-1",
      items: [message],
      has_before: false,
      has_messages: true,
      total_count: 1,
      version: 1,
    },
    active_requests: [],
    send_capability: { state: "ready" },
    settings_summary: { agent_id: "codex", isolation: "local" },
    revision: 1,
  };
}
