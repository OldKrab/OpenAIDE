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
    expect(jumpButtons(tree)).toHaveLength(1);

    act(() => {
      messageList.scrollTop = 1150;
      messageListView(tree).props.onScroll({ currentTarget: messageList });
    });

    expect(jumpButtons(tree)).toHaveLength(0);

    act(() => {
      messageList.scrollTop = 1098;
      messageListView(tree).props.onScroll({ currentTarget: messageList });
      messageListView(tree).props.onWheel({ deltaY: 8 });
      messageList.scrollTop = 1150;
      messageListView(tree).props.onScroll({ currentTarget: messageList });
    });

    expect(messageList.scrollTop).toBe(1150);
    expect(jumpButtons(tree)).toHaveLength(0);

    messageList.scrollHeight = 1600;
    act(() => tree.update(<TaskView {...taskViewProps(snapshot("active", 4))} />));

    expect(messageList.scrollTop).toBe(1200);
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
});

function messageListView(tree: ReactTestRenderer) {
  return tree.root.findByProps({ className: "message-list" });
}

function jumpButtons(tree: ReactTestRenderer) {
  return tree.root.findAllByProps({ className: "jump-to-latest" });
}

function taskViewProps(taskSnapshot: TaskSnapshot) {
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
    snapshot: taskSnapshot,
    taskInput: { prompt: "", context: [] },
    toolDetails: {},
    submitShortcut: "mod_enter" as const,
  };
}

function snapshot(status: TaskSnapshot["task"]["status"], revision = 1, taskId = "task-1"): TaskSnapshot {
  return {
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
    chat: {
      task_id: taskId,
      items: [],
      has_before: false,
      has_messages: false,
      total_count: revision,
      version: revision,
    },
    permissions: [],
    send_capability: { state: "ready", attachment_only: true },
    settings_summary: { agent_id: "codex", isolation: "local" },
    revision,
  };
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
