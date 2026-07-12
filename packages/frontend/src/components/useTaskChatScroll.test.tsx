import { useState } from "react";
import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useTaskChatScroll } from "./useTaskChatScroll";

describe("useTaskChatScroll", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    vi.stubGlobal("sessionStorage", memoryStorage());
  });

  afterEach(() => vi.unstubAllGlobals());

  it("freezes a Task-scoped diagnostic trace when layout movement strands a follower", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    messageList.scrollHeight = 800;
    messageList.scrollHeight = 2000;
    act(() => {
      tree.root.findByProps({ className: "message-list" }).props.onScroll({ currentTarget: messageList });
    });

    const trace = JSON.parse(sessionStorage.getItem("openaide:scroll-diagnostics:task_1") ?? "null");
    expect(trace).toMatchObject({ frozen: true, taskId: "task_1" });
    expect(trace.events.at(-1)).toMatchObject({
      type: "anomaly",
      geometry: { scrollTop: 400, scrollHeight: 2000, clientHeight: 400, distanceFromBottom: 1200 },
    });
    expect(trace.events).toContainEqual(expect.objectContaining({
      type: "render",
      context: {
        chatVersion: 7,
        historySyncState: "idle",
        itemCount: 1,
        itemKindCounts: { permission: 1 },
        olderItemCount: 3,
        pendingPermissions: ["permission-1"],
        snapshotRevision: 9,
        taskStatus: "blocked",
      },
    }));
    expect(trace.events).toContainEqual(expect.objectContaining({
      type: "ownership",
      ownership: "reading",
      reason: "scrollTopDecreased",
    }));
    expect(JSON.stringify(trace)).not.toContain("messageText");
  });

  it("does not freeze diagnostics after explicit upward wheel intent", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    messageList.scrollHeight = 800;
    messageList.scrollHeight = 2000;
    act(() => {
      const viewport = tree.root.findByProps({ className: "message-list" });
      viewport.props.onWheel({ deltaY: -12 });
      viewport.props.onScroll({ currentTarget: messageList });
    });

    const trace = JSON.parse(sessionStorage.getItem("openaide:scroll-diagnostics:task_1") ?? "null");
    expect(trace).toMatchObject({ frozen: false, taskId: "task_1" });
  });

  it("records Task viewport mount and unmount boundaries", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });
    act(() => tree.unmount());

    const trace = JSON.parse(sessionStorage.getItem("openaide:scroll-diagnostics:task_1") ?? "null");
    expect(trace.events.filter((event: { type: string }) => event.type === "lifecycle")).toEqual([
      expect.objectContaining({ state: "mounted" }),
      expect.objectContaining({ state: "unmounted" }),
    ]);
  });

  it("keeps follow mode off when persisting a small upward scroll", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    messageList.scrollTop = 998;
    act(() => {
      const viewport = tree.root.findByProps({ className: "message-list" });
      viewport.props.onWheel({ deltaY: -2 });
      viewport.props.onScroll({ currentTarget: messageList });
    });

    expect(messageList.scrollTop).toBe(998);
    messageList.scrollHeight = 1500;
    act(() => tree.update(<Harness itemCount={2} />));

    expect(messageList.scrollTop).toBe(998);
  });

  it("follows final output when the Task becomes inactive in the same update", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness generating itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    expect(messageList.scrollTop).toBe(1000);
    messageList.scrollHeight = 1500;
    act(() => tree.update(<Harness generating={false} itemCount={2} />));

    expect(messageList.scrollTop).toBe(1100);
  });

  it("keeps following when layout contraction clamps the viewport without user input", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    messageList.scrollHeight = 1350;
    act(() => {
      tree.root.findByProps({ className: "message-list" }).props.onScroll({ currentTarget: messageList });
    });
    messageList.scrollHeight = 1450;
    act(() => tree.update(<Harness itemCount={2} />));

    expect(messageList.scrollTop).toBe(1050);
  });

  it("lets a slow touch swipe leave follow mode from its first small movement", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    messageList.scrollTop = 999;
    act(() => {
      const viewport = tree.root.findByProps({ className: "message-list" });
      viewport.props.onPointerDown({ pointerType: "touch" });
      viewport.props.onScroll({ currentTarget: messageList });
    });
    messageList.scrollHeight = 1500;
    act(() => tree.update(<Harness itemCount={2} />));

    expect(messageList.scrollTop).toBe(999);
  });

  it("keeps a reader anchored when synchronized history is inserted", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<Harness historySyncState="checking" itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });
    messageList.scrollTop = 700;
    act(() => {
      const viewport = tree.root.findByProps({ className: "message-list" });
      viewport.props.onWheel({ deltaY: -10 });
      viewport.props.onScroll({ currentTarget: messageList });
    });

    messageList.scrollHeight = 1600;
    act(() => tree.update(<Harness historySyncState="updated" itemCount={2} />));

    expect(messageList.scrollTop).toBe(900);
  });

  it("stays at the bottom when synchronized history is inserted while following", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<Harness historySyncState="checking" itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    messageList.scrollHeight = 1600;
    act(() => tree.update(<Harness historySyncState="updated" itemCount={2} />));

    expect(messageList.scrollTop).toBe(1200);
  });
});

function Harness({
  generating = true,
  historySyncState = "idle",
  itemCount,
}: {
  generating?: boolean;
  historySyncState?: "idle" | "checking" | "syncing" | "updated" | "failed";
  itemCount: number;
}) {
  const [savedScrollTop, setSavedScrollTop] = useState(1000);
  const chatScroll = useTaskChatScroll({
    diagnosticContext: {
      chatVersion: 7,
      historySyncState,
      itemCount,
      itemKindCounts: { permission: 1 },
      olderItemCount: 3,
      pendingPermissions: ["permission-1"],
      snapshotRevision: 9,
      taskStatus: "blocked",
    },
    generating,
    historySyncState,
    itemCount,
    onScrollTop: setSavedScrollTop,
    pendingPrepend: false,
    savedScrollTop,
    taskId: "task_1",
  });

  return (
    <div
      className="message-list"
      onPointerDown={chatScroll.onPointerDown}
      onScroll={chatScroll.onScroll}
      onWheel={chatScroll.onWheel}
      ref={chatScroll.messageListRef}
    />
  );
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
      currentScrollTop = Math.min(currentScrollTop, currentScrollHeight - clientHeight);
    },
    get scrollTop() {
      return currentScrollTop;
    },
    set scrollTop(nextScrollTop: number) {
      currentScrollTop = Math.max(0, Math.min(nextScrollTop, currentScrollHeight - clientHeight));
    },
  };
}

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => values.delete(key),
    setItem: (key, value) => values.set(key, value),
  };
}
