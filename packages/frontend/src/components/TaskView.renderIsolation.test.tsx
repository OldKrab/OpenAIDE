import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatMessage, TaskSnapshot } from "@openaide/app-shell-contracts";

const chatRowRender = vi.hoisted(() => vi.fn());

vi.mock("./ChatMessageView", () => ({
  ChatRow: ({ message }: { message: ChatMessage }) => {
    chatRowRender(message.message_id);
    return <p data-chat-row-id={message.message_id}>{message.message_id}</p>;
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

  it("mounts only a viewport-sized subset of a long Chat history", () => {
    const snapshot = taskSnapshot();
    snapshot.chat.items = Array.from({ length: 200 }, (_, index) => chatMessage(index));
    snapshot.chat.total_count = snapshot.chat.items.length;
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<TaskView {...taskViewProps(snapshot)} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list"
            ? scrollViewport(600)
            : null
        ),
      });
    });

    const mountedRows = tree.root.findAll((node) => node.props["data-chat-row-id"] !== undefined);
    expect(mountedRows.length).toBeGreaterThan(0);
    expect(mountedRows.length).toBeLessThan(40);
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

  it("renders Plan beside Chat and anchors Queue above Composer", () => {
    const snapshot = taskSnapshot();
    snapshot.current_plan = {
      entries: [{ content: "Verify layout", priority: "medium", status: "in_progress" }],
    };
    snapshot.message_queue = {
      revision: 2,
      items: [
        { queued_message_id: "queued-1", text: "Next", created_at: "now" },
        { queued_message_id: "queued-2", text: "Later", created_at: "now" },
      ],
    };
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<TaskView
        {...taskViewProps(snapshot)}
        onMoveQueueMessage={vi.fn()}
        onRemoveQueueMessage={vi.fn()}
        onTakeQueueMessage={vi.fn()}
      />);
    });

    const conversation = tree.root.findByProps({ className: "chat-column task-conversation" });
    expect(conversation.findAllByProps({ className: "task-plan-column" })).toHaveLength(1);
    expect(tree.root.findAllByProps({ className: "task-plan-drawer" })).toHaveLength(1);
    expect(tree.root.findByProps({ className: "task-queue-anchor" })
      .findByProps({ className: "task-message-queue" }).props["data-single"]).toBeUndefined();
  });

  it("opens Plan when the Agent creates one during the Task", () => {
    const snapshot = taskSnapshot();
    const onPlanDrawerOpenChange = vi.fn();
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<TaskView {...taskViewProps(snapshot)} onPlanDrawerOpenChange={onPlanDrawerOpenChange} />);
    });
    expect(onPlanDrawerOpenChange).not.toHaveBeenCalled();

    snapshot.current_plan = {
      entries: [{ content: "Write the patch", priority: "medium", status: "pending" }],
    };
    act(() => {
      tree.update(<TaskView {...taskViewProps(snapshot)} onPlanDrawerOpenChange={onPlanDrawerOpenChange} />);
    });
    expect(onPlanDrawerOpenChange).toHaveBeenCalledWith(true);
  });

  it("does not reopen Plan when the Task already has one", () => {
    const snapshot = taskSnapshot();
    snapshot.current_plan = {
      entries: [{ content: "Existing", priority: "medium", status: "in_progress" }],
    };
    const onPlanDrawerOpenChange = vi.fn();
    act(() => {
      create(<TaskView {...taskViewProps(snapshot)} onPlanDrawerOpenChange={onPlanDrawerOpenChange} />);
    });
    expect(onPlanDrawerOpenChange).not.toHaveBeenCalled();
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
  const message = chatMessage(1);
  return {
    lifecycle: "open",
    permission_policy: "ask_every_time",
    task: {
      task_id: "task-1",
      title: "Task",
      status: "inactive",
      task_version: 1,
      message_history_version: 1,
      has_messages: true,
      unread: false,
      pinned: false,
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

function chatMessage(index: number): ChatMessage {
  const messageId = `agent-${index}`;
  return {
    cursor: messageId,
    identity: messageId,
    message_id: messageId,
    message_type: "agent_message",
    message: {
      kind: "agent_message",
      id: messageId,
      role: "agent",
      parts: [{ kind: "text", text: "Stable answer" }],
      created_at: "2026-07-13T00:00:00Z",
    },
  };
}

function scrollViewport(clientHeight: number) {
  return {
    addEventListener: vi.fn(),
    clientHeight,
    clientWidth: 800,
    getBoundingClientRect: () => ({ height: clientHeight, width: 800 }),
    removeEventListener: vi.fn(),
    scrollHeight: 20_000,
    scrollTo: vi.fn(),
    scrollTop: 0,
  };
}
