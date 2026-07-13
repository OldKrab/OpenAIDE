import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";

describe("TaskView follow scroll", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    vi.stubGlobal("window", { acquireVsCodeApi: undefined });
  });

  afterEach(() => vi.unstubAllGlobals());

  it("keeps scroll ownership with the reader until they return to latest", async () => {
    const { TaskView } = await import("./TaskView");
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<TaskView {...taskViewProps(snapshot("active"))} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    expect(messageList.scrollTop).toBe(1000);
    expect(jumpButtons(tree)).toHaveLength(0);

    messageList.scrollHeight = 1500;
    act(() => tree.update(<TaskView {...taskViewProps(snapshot("active", 2))} />));

    expect(messageList.scrollTop).toBe(1100);
    expect(jumpButtons(tree)).toHaveLength(0);

    act(() => {
      messageListView(tree).props.onWheel({ deltaY: -8 });
      messageList.scrollTop = 1098;
      messageListView(tree).props.onScroll({ currentTarget: messageList });
    });

    expect(messageList.scrollTop).toBe(1098);
    expect(jumpButtons(tree)).toHaveLength(0);

    messageList.scrollHeight = 1550;
    act(() => tree.update(<TaskView {...taskViewProps(snapshot("active", 3))} />));

    expect(messageList.scrollTop).toBe(1098);
    expect(jumpButtons(tree)).toHaveLength(0);

    messageList.scrollHeight = 1650;
    act(() => tree.update(<TaskView {...taskViewProps(snapshot("active", 4))} />));

    expect(jumpButtons(tree)).toHaveLength(1);
    expect(jumpButtons(tree)[0].props["aria-label"]).toBe("Jump to latest message");
    expect(jumpButtons(tree)[0].props.title).toBe("Jump to latest");
    expect(jumpButtons(tree)[0].findByType("svg").props["aria-hidden"]).toBe("true");

    act(() => {
      messageList.scrollTop = 1250;
      messageListView(tree).props.onScroll({ currentTarget: messageList });
    });

    expect(jumpButtons(tree)).toHaveLength(0);

    act(() => {
      messageList.scrollTop = 1198;
      messageListView(tree).props.onScroll({ currentTarget: messageList });
      messageListView(tree).props.onWheel({ deltaY: 8 });
      messageList.scrollTop = 1250;
      messageListView(tree).props.onScroll({ currentTarget: messageList });
    });

    expect(messageList.scrollTop).toBe(1250);
    expect(jumpButtons(tree)).toHaveLength(0);

    messageList.scrollHeight = 1700;
    act(() => tree.update(<TaskView {...taskViewProps(snapshot("active", 5))} />));

    expect(messageList.scrollTop).toBe(1300);
    expect(jumpButtons(tree)).toHaveLength(0);
  });

  it("focuses the composer again when the user switches tasks", async () => {
    const { TaskView } = await import("./TaskView");
    const focus = vi.fn();
    const editor = {
      focus,
      innerHTML: "",
      ownerDocument: { activeElement: null },
    };
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<TaskView {...taskViewProps(snapshot("inactive", 1, "task-1"))} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "composer-editor" ? editor : null
        ),
      });
    });
    const focusCallsAfterFirstTask = focus.mock.calls.length;

    act(() => tree.update(<TaskView {...taskViewProps(snapshot("inactive", 1, "task-2"))} />));

    expect(focusCallsAfterFirstTask).toBeGreaterThan(0);
    expect(focus.mock.calls.length).toBeGreaterThan(focusCallsAfterFirstTask);
  });

  it("shows both buffered and newly received text immediately", async () => {
    const { TaskView } = await import("./TaskView");
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<TaskView {...taskViewProps(snapshot("active", 1, "task-1"))} />);
    });
    act(() => tree.update(<TaskView {...taskViewProps(snapshot("active", 1, "task-2"))} />));
    act(() => {
      tree.update(
        <TaskView
          {...taskViewProps(snapshotWithStreamingText("task-1", "Buffered while away."))}
        />,
      );
    });

    expect(JSON.stringify(tree.toJSON())).toContain("Buffered while away.");

    act(() => {
      tree.update(
        <TaskView
          {...taskViewProps(snapshotWithStreamingText("task-1", "Buffered while away. Newly received."))}
        />,
      );
    });

    expect(JSON.stringify(tree.toJSON())).toContain("Newly received.");
  });

  it("smoothly returns to the latest message when the user clicks the jump control", async () => {
    const { TaskView } = await import("./TaskView");
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    const animationFrames: FrameRequestCallback[] = [];
    let now = 0;
    vi.stubGlobal("performance", { now: () => now });
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      animationFrames.push(callback);
      return animationFrames.length;
    });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<TaskView {...taskViewProps(snapshot("inactive"))} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });
    act(() => {
      messageListView(tree).props.onWheel({ deltaY: -8 });
      messageList.scrollTop = 800;
      messageListView(tree).props.onScroll({ currentTarget: messageList });
    });

    act(() => jumpButtons(tree)[0].props.onClick());
    expect(messageList.scrollTop).toBe(800);

    now = 90;
    act(() => animationFrames.shift()?.(now));
    expect(messageList.scrollTop).toBeGreaterThan(800);
    expect(messageList.scrollTop).toBeLessThan(1000);

    now = 180;
    act(() => animationFrames.shift()?.(now));
    expect(messageList.scrollTop).toBe(1000);
  });

  it("keeps a pending follow-up in the disabled composer without changing Chat", async () => {
    const { TaskView } = await import("./TaskView");
    const props = taskViewProps(snapshot("inactive"));
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(
        <TaskView
          {...props}
          taskInput={{
            prompt: "",
            context: [],
            pending: { prompt: "Ship the follow-up", context: [], state: "sending" },
          }}
        />,
      );
    });

    expect(tree.root.findByProps({ className: "composer-editor" }).props["aria-disabled"]).toBe(true);
    expect(tree.root.findByProps({ className: "composer-submit-pending" })).toBeTruthy();
    expect(tree.root.findAllByProps({ className: "working-status" })).toHaveLength(0);
  });
});

function messageListView(tree: ReactTestRenderer) {
  return tree.root.findByProps({ className: "message-list" });
}

function jumpButtons(tree: ReactTestRenderer) {
  return tree.root.findAllByProps({ className: "jump-to-latest" });
}

function taskViewProps(taskSnapshot: TaskSnapshot) {
  return {
    backendReady: true,
    chatPageState: undefined,
    dispatch: vi.fn(),
    onCancel: vi.fn(),
    onLoadChatPage: vi.fn(),
    onSubscribeToolDetail: vi.fn(() => vi.fn()),
    onPermissionRespond: vi.fn(),
    onRevealAttachment: vi.fn(),
    onRemoveAttachment: vi.fn(),
    onSendPrompt: vi.fn(),
    onSelectConfigOption: vi.fn(),
    permissionResponses: {},
    snapshot: taskSnapshot,
    taskInput: { prompt: "", context: [] },
    toolDetails: {},
    submitShortcut: "mod_enter" as const,
  };
}

function snapshot(status: TaskSnapshot["task"]["status"], revision = 1, taskId = "task-1"): TaskSnapshot {
  return {
    lifecycle: "visible",
    task: {
      task_id: taskId,
      title: "Task",
      status,
      task_version: revision,
      message_history_version: revision,
      has_messages: false,
      unread: false,
      created_at: "2026-07-10T00:00:00Z",
      updated_at: "2026-07-10T00:00:00Z",
      last_activity: "2026-07-10T00:00:00Z",
      agent_id: "codex",
      agent_name: "Codex",
      isolation: "local",
      workspace_root: "/workspace",
    },
    history_sync: { state: "idle", generation: 0 },
    chat: {
      task_id: taskId,
      items: [],
      has_before: false,
      has_messages: false,
      total_count: revision,
      version: revision,
    },
    active_requests: [],
    send_capability: { state: "ready" },
    settings_summary: { agent_id: "codex", isolation: "local" },
    revision,
  };
}

function snapshotWithStreamingText(taskId: string, text: string): TaskSnapshot {
  const taskSnapshot = snapshot("active", 2, taskId);
  taskSnapshot.task.has_messages = true;
  taskSnapshot.chat.has_messages = true;
  taskSnapshot.chat.items = [{
    cursor: "message-1",
    identity: "message-1",
    message_type: "agent_message",
    message_id: "message-1",
    message: {
      kind: "agent_message",
      id: "agent-1",
      role: "agent",
      parts: [{ kind: "text", text }],
      created_at: "2026-07-10T00:00:00Z",
    },
  }];
  return taskSnapshot;
}

function scrollNode({ clientHeight, scrollHeight }: { clientHeight: number; scrollHeight: number }) {
  let currentScrollHeight = scrollHeight;
  let currentScrollTop = 0;
  return {
    clientHeight,
    get scrollHeight() {
      return currentScrollHeight;
    },
    set scrollHeight(nextScrollHeight: number) {
      currentScrollHeight = nextScrollHeight;
    },
    get scrollTop() {
      return currentScrollTop;
    },
    set scrollTop(nextScrollTop: number) {
      currentScrollTop = Math.max(0, Math.min(nextScrollTop, currentScrollHeight - clientHeight));
    },
  };
}
