import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";

const virtualizerCalls = vi.hoisted(() => ({ scrollToEnd: vi.fn() }));

vi.mock("@tanstack/react-virtual", async () => {
  const React = await import("react");
  type Options = {
    count: number;
    getItemKey: (index: number) => string;
    getScrollElement: () => HTMLDivElement | null;
  };
  return {
    useVirtualizer: (options: Options) => {
      const optionsRef = React.useRef(options);
      optionsRef.current = options;
      const virtualizerRef = React.useRef<ReturnType<typeof createVirtualizer> | undefined>(undefined);
      virtualizerRef.current ??= createVirtualizer(optionsRef);
      return virtualizerRef.current;
    },
  };

  function createVirtualizer(optionsRef: { current: Options }) {
    const element = () => optionsRef.current.getScrollElement();
    const distanceFromEnd = () => {
      const viewport = element();
      return viewport ? Math.max(0, viewport.scrollHeight - viewport.clientHeight - viewport.scrollTop) : 0;
    };
    return {
      getDistanceFromEnd: distanceFromEnd,
      getTotalSize: () => element()?.scrollHeight ?? optionsRef.current.count * 72,
      getVirtualItems: () => Array.from({ length: optionsRef.current.count }, (_, index) => ({
        end: (index + 1) * 72,
        index,
        key: optionsRef.current.getItemKey(index),
        lane: 0,
        size: 72,
        start: index * 72,
      })),
      isAtEnd: (threshold = 2) => distanceFromEnd() <= threshold,
      measureElement: () => undefined,
      scrollToEnd: (options?: { behavior?: ScrollBehavior }) => {
        virtualizerCalls.scrollToEnd(options);
        const viewport = element();
        if (viewport) viewport.scrollTop = Math.max(0, viewport.scrollHeight - viewport.clientHeight);
      },
      scrollToOffset: (offset: number) => {
        const viewport = element();
        if (viewport) viewport.scrollTop = offset;
      },
    };
  }
});

describe("TaskView follow scroll", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    vi.stubGlobal("window", { acquireVsCodeApi: undefined });
    virtualizerCalls.scrollToEnd.mockClear();
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

  it("loads an earlier page when folded Chat does not fill the viewport", async () => {
    const { TaskView } = await import("./TaskView");
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 180 });
    const taskSnapshot = snapshotWithStreamingText("task-1", "Recent response");
    taskSnapshot.task.status = "inactive";
    taskSnapshot.chat.has_before = true;
    taskSnapshot.chat.start_cursor = "cursor-oldest-loaded";
    const onLoadChatPage = vi.fn(() => 1);

    act(() => {
      create(
        <TaskView
          {...taskViewProps(taskSnapshot)}
          onLoadChatPage={onLoadChatPage}
        />,
        {
          createNodeMock: (element) => (
            (element.props as { className?: string }).className === "message-list" ? messageList : null
          ),
        },
      );
    });

    expect(onLoadChatPage).toHaveBeenCalledTimes(1);
    expect(onLoadChatPage).toHaveBeenCalledWith("cursor-oldest-loaded");
  });

  it("keeps earlier history manual when the rendered Chat already fills the viewport", async () => {
    const { TaskView } = await import("./TaskView");
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 640 });
    const taskSnapshot = snapshotWithStreamingText("task-1", "Recent response");
    taskSnapshot.chat.has_before = true;
    taskSnapshot.chat.start_cursor = "cursor-oldest-loaded";
    const onLoadChatPage = vi.fn(() => 1);

    act(() => {
      create(
        <TaskView
          {...taskViewProps(taskSnapshot)}
          onLoadChatPage={onLoadChatPage}
        />,
        {
          createNodeMock: (element) => (
            (element.props as { className?: string }).className === "message-list" ? messageList : null
          ),
        },
      );
    });

    expect(onLoadChatPage).not.toHaveBeenCalled();
  });

  it("bounds automatic history loading when collapsed pages keep Chat underfilled", async () => {
    const { TaskView } = await import("./TaskView");
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 180 });
    const taskSnapshot = snapshotWithStreamingText("task-1", "Recent response");
    taskSnapshot.chat.has_before = true;
    taskSnapshot.chat.start_cursor = "cursor-0";
    const onLoadChatPage = vi.fn((_beforeCursor: string) => onLoadChatPage.mock.calls.length);
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(
        <TaskView
          {...taskViewProps(taskSnapshot)}
          onLoadChatPage={onLoadChatPage}
        />,
        {
          createNodeMock: (element) => (
            (element.props as { className?: string }).className === "message-list" ? messageList : null
          ),
        },
      );
    });

    for (let page = 1; page <= 5; page += 1) {
      act(() => {
        tree.update(
          <TaskView
            {...taskViewProps(taskSnapshot)}
            chatPageState={{
              olderItems: [],
              hasBefore: true,
              pending: false,
              requestGeneration: page,
              startCursor: `cursor-${page}`,
            }}
            onLoadChatPage={onLoadChatPage}
          />,
        );
      });
    }

    expect(onLoadChatPage.mock.calls.map(([cursor]) => cursor)).toEqual([
      "cursor-0",
      "cursor-1",
      "cursor-2",
      "cursor-3",
    ]);
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

    virtualizerCalls.scrollToEnd.mockClear();
    act(() => jumpButtons(tree)[0].props.onClick());
    expect(virtualizerCalls.scrollToEnd).toHaveBeenCalledWith({ behavior: "smooth" });
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
    lifecycle: "open",
    task: {
      task_id: taskId,
      title: "Task",
      status,
      task_version: revision,
      message_history_version: revision,
      has_messages: false,
      unread: false,
      pinned: false,
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
