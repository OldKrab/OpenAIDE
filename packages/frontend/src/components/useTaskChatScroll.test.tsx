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

  it("recovers a follower when permission layout contracts and grows before its scroll event", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    act(() => tree.update(<Harness itemCount={1} pendingPermissions={[]} />));
    messageList.scrollHeight = 800;
    messageList.scrollHeight = 2000;
    act(() => {
      tree.root.findByProps({ className: "message-list" }).props.onScroll({ currentTarget: messageList });
    });

    expect(messageList.scrollTop).toBe(1600);
    const trace = JSON.parse(sessionStorage.getItem("openaide:scroll-diagnostics:task_1") ?? "null");
    expect(trace).toMatchObject({ frozen: false, taskId: "task_1" });
    expect(trace.events).not.toContainEqual(expect.objectContaining({ type: "anomaly" }));
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
    expect(trace.events).not.toContainEqual(expect.objectContaining({
      type: "ownership",
      ownership: "reading",
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

  it("keeps an unclassified reader scroll on stable geometry", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} pendingPermissions={[]} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    messageList.scrollTop = 700;
    act(() => {
      tree.root.findByProps({ className: "message-list" }).props.onScroll({ currentTarget: messageList });
    });

    expect(messageList.scrollTop).toBe(700);
    messageList.scrollHeight = 1500;
    act(() => tree.update(<Harness itemCount={2} pendingPermissions={[]} />));
    expect(messageList.scrollTop).toBe(700);
  });

  it("preserves keyboard reader intent after a permission resolves", () => {
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
    act(() => tree.update(<Harness itemCount={1} pendingPermissions={[]} />));

    const viewport = tree.root.findByProps({ className: "message-list" });
    messageList.scrollTop = 700;
    act(() => {
      viewport.props.onKeyDown({
        altKey: false,
        ctrlKey: false,
        currentTarget: messageList,
        defaultPrevented: false,
        key: "PageUp",
        metaKey: false,
        shiftKey: false,
        target: messageList,
      });
      viewport.props.onScroll({ currentTarget: messageList });
    });

    messageList.scrollHeight = 2100;
    act(() => tree.update(<Harness itemCount={2} pendingPermissions={[]} />));
    expect(messageList.scrollTop).toBe(700);
  });

  it("expires permission layout recovery after the resolution paint", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let afterPaint: FrameRequestCallback | undefined;
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      afterPaint = callback;
      return 1;
    });
    vi.stubGlobal("cancelAnimationFrame", vi.fn());
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    act(() => tree.update(<Harness itemCount={1} pendingPermissions={[]} />));
    expect(afterPaint).toBeTypeOf("function");
    afterPaint?.(16);

    messageList.scrollHeight = 800;
    messageList.scrollHeight = 2000;
    act(() => {
      tree.root.findByProps({ className: "message-list" }).props.onScroll({ currentTarget: messageList });
    });

    expect(messageList.scrollTop).toBe(400);
  });

  it("restores following when keyboard navigation reaches the latest message", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} pendingPermissions={[]} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    const viewport = tree.root.findByProps({ className: "message-list" });
    messageList.scrollTop = 700;
    act(() => {
      viewport.props.onWheel({ deltaY: -20 });
      viewport.props.onScroll({ currentTarget: messageList });
      viewport.props.onKeyDown({
        altKey: false,
        ctrlKey: false,
        currentTarget: messageList,
        defaultPrevented: false,
        key: "End",
        metaKey: false,
        shiftKey: false,
        target: messageList,
      });
      messageList.scrollTop = 1000;
      viewport.props.onScroll({ currentTarget: messageList });
    });

    messageList.scrollHeight = 1500;
    act(() => tree.update(<Harness itemCount={2} pendingPermissions={[]} />));
    expect(messageList.scrollTop).toBe(1100);
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

  it("lets a mouse scrollbar drag leave follow mode", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    messageList.scrollTop = 700;
    act(() => {
      const viewport = tree.root.findByProps({ className: "message-list" });
      viewport.props.onPointerDown({ pointerType: "mouse" });
      viewport.props.onScroll({ currentTarget: messageList });
      viewport.props.onPointerUp({ pointerType: "mouse" });
    });
    messageList.scrollHeight = 1500;
    act(() => tree.update(<Harness itemCount={2} />));

    expect(messageList.scrollTop).toBe(700);
  });

  it("ends pointer ownership when the pointer is released outside Chat", () => {
    const windowListeners = new Map<string, EventListener>();
    vi.stubGlobal("window", {
      addEventListener: (type: string, listener: EventListener) => windowListeners.set(type, listener),
      removeEventListener: (type: string) => windowListeners.delete(type),
    });
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    const viewport = tree.root.findByProps({ className: "message-list" });
    act(() => {
      viewport.props.onPointerDown({ pointerType: "mouse" });
      windowListeners.get("pointerup")?.(new Event("pointerup"));
      tree.update(<Harness itemCount={1} pendingPermissions={[]} />);
    });
    messageList.scrollHeight = 800;
    messageList.scrollHeight = 2000;
    act(() => viewport.props.onScroll({ currentTarget: messageList }));

    expect(messageList.scrollTop).toBe(1600);
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
  pendingPermissions = ["permission-1"],
}: {
  generating?: boolean;
  historySyncState?: "idle" | "checking" | "syncing" | "updated" | "failed";
  itemCount: number;
  pendingPermissions?: string[];
}) {
  const [savedScrollTop, setSavedScrollTop] = useState(1000);
  const chatScroll = useTaskChatScroll({
    diagnosticContext: {
      chatVersion: 7,
      historySyncState,
      itemCount,
      itemKindCounts: { permission: 1 },
      olderItemCount: 3,
      pendingPermissions,
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
      onPointerCancel={chatScroll.onPointerCancel}
      onPointerDown={chatScroll.onPointerDown}
      onPointerUp={chatScroll.onPointerUp}
      onKeyDown={chatScroll.onKeyDown}
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
