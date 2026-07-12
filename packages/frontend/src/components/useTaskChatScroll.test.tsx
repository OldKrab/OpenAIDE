import { useState } from "react";
import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskChatScrollState } from "../state/store";
import { useTaskChatScroll } from "./useTaskChatScroll";

describe("useTaskChatScroll", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("keeps a following viewport pinned through activity, permission, and later content reflow", () => {
    const resize = installResizeObserver();
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    messageList.scrollHeight = 2200;
    act(() => resize.notify());
    expect(messageList.scrollTop).toBe(1800);

    act(() => tree.update(<Harness itemCount={2} />));
    messageList.scrollHeight = 2600;
    act(() => resize.notify());
    expect(messageList.scrollTop).toBe(2200);

    act(() => tree.update(<Harness itemCount={2} />));
    messageList.scrollHeight = 2100;
    act(() => resize.notify());
    expect(messageList.scrollTop).toBe(1700);

    // Markdown, images, and tool details can settle without another React render.
    messageList.scrollHeight = 2900;
    act(() => resize.notify());
    expect(messageList.scrollTop).toBe(2500);
  });

  it("restores a Task's saved following ownership after its Chat grows in the background", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} taskId="task_1" />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });
    expect(messageList.scrollTop).toBe(1000);

    act(() => tree.update(<Harness itemCount={1} taskId="task_2" />));
    messageList.scrollHeight = 1800;
    act(() => tree.update(<Harness itemCount={2} taskId="task_1" />));

    expect(messageList.scrollTop).toBe(1400);
  });

  it("observes Chat rows inserted after mount for later intrinsic-size changes", () => {
    const resize = installResizeObserver();
    const mutation = installMutationObserver();
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    const insertedRow = {} as Element;
    messageList.children.push(insertedRow);
    act(() => mutation.notify());

    expect(resize.instances[0]?.observe).toHaveBeenCalledWith(insertedRow);
  });

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

    act(() => tree.update(<Harness itemCount={1} />));
    messageList.scrollHeight = 800;
    messageList.scrollHeight = 2000;
    act(() => {
      tree.root.findByProps({ className: "message-list" }).props.onScroll({ currentTarget: messageList });
    });

    expect(messageList.scrollTop).toBe(1600);
  });

  it("cancels observers, listeners, and animation frames when the Task changes", () => {
    const resize = installResizeObserver();
    const mutation = installMutationObserver();
    const addEventListener = vi.fn();
    const removeEventListener = vi.fn();
    vi.stubGlobal("window", {
      addEventListener,
      matchMedia: () => ({ matches: false }),
      removeEventListener,
    });
    vi.stubGlobal("performance", { now: () => 0 });
    vi.stubGlobal("requestAnimationFrame", vi.fn(() => 41));
    const cancelAnimationFrame = vi.fn();
    vi.stubGlobal("cancelAnimationFrame", cancelAnimationFrame);
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} taskId="task_1" />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });
    act(() => {
      const viewport = tree.root.findByProps({ className: "message-list" });
      viewport.props.onWheel({ deltaY: -10 });
      messageList.scrollTop = 700;
      viewport.props.onScroll({ currentTarget: messageList });
      tree.root.findByProps({ className: "jump" }).props.onClick();
    });

    act(() => tree.update(<Harness itemCount={1} taskId="task_2" />));

    expect(resize.instances[0]?.disconnect).toHaveBeenCalledOnce();
    expect(mutation.instances[0]?.disconnect).toHaveBeenCalledOnce();
    expect(removeEventListener).toHaveBeenCalledWith("pointerup", expect.any(Function));
    expect(removeEventListener).toHaveBeenCalledWith("pointercancel", expect.any(Function));
    expect(removeEventListener).toHaveBeenCalledWith("pointermove", expect.any(Function));
    expect(cancelAnimationFrame).toHaveBeenCalledWith(41);

    act(() => tree.unmount());
    expect(resize.instances[1]?.disconnect).toHaveBeenCalledOnce();
    expect(mutation.instances[1]?.disconnect).toHaveBeenCalledOnce();
    expect(removeEventListener.mock.calls.filter(([type]) => type === "pointerup")).toHaveLength(2);
    expect(removeEventListener.mock.calls.filter(([type]) => type === "pointercancel")).toHaveLength(2);
    expect(removeEventListener.mock.calls.filter(([type]) => type === "pointermove")).toHaveLength(2);
  });

  it("keeps follow mode off when persisting a small upward scroll", () => {
    const resize = installResizeObserver();
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
    act(() => resize.notify());

    expect(messageList.scrollTop).toBe(998);
  });

  it("does not treat an unclassified scroll event as reader intent", () => {
    const resize = installResizeObserver();
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
      tree.root.findByProps({ className: "message-list" }).props.onScroll({ currentTarget: messageList });
    });

    expect(messageList.scrollTop).toBe(1000);
    messageList.scrollHeight = 1500;
    act(() => resize.notify());
    expect(messageList.scrollTop).toBe(1100);
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
    act(() => tree.update(<Harness itemCount={1} />));

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
    act(() => tree.update(<Harness itemCount={2} />));
    expect(messageList.scrollTop).toBe(700);
  });

  it("restores following when keyboard navigation reaches the latest message", () => {
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
    act(() => tree.update(<Harness itemCount={2} />));
    expect(messageList.scrollTop).toBe(1100);
  });

  it("restores following from explicit downward intent without a timing window", () => {
    const resize = installResizeObserver();
    let now = 0;
    vi.spyOn(Date, "now").mockImplementation(() => now);
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
      viewport.props.onWheel({ deltaY: -20 });
      messageList.scrollTop = 700;
      viewport.props.onScroll({ currentTarget: messageList });
      viewport.props.onWheel({ deltaY: 20 });
    });
    now = 10_000;
    act(() => {
      messageList.scrollTop = 1000;
      viewport.props.onScroll({ currentTarget: messageList });
    });

    messageList.scrollHeight = 1500;
    act(() => resize.notify());
    expect(messageList.scrollTop).toBe(1100);
  });

  it("restores following when the user wheels down from an already-bottom reader viewport", () => {
    const resize = installResizeObserver();
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
      viewport.props.onWheel({ deltaY: -10 });
      messageList.scrollTop = 700;
      viewport.props.onScroll({ currentTarget: messageList });
      messageList.scrollTop = 1000;
      viewport.props.onScroll({ currentTarget: messageList });
      viewport.props.onWheel({ currentTarget: messageList, deltaY: 10 });
    });

    messageList.scrollHeight = 1500;
    act(() => resize.notify());
    expect(messageList.scrollTop).toBe(1100);
  });

  it("follows final output when the Task becomes inactive in the same update", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    expect(messageList.scrollTop).toBe(1000);
    messageList.scrollHeight = 1500;
    act(() => tree.update(<Harness itemCount={2} />));

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

    messageList.scrollTop = 999;
    act(() => {
      const viewport = tree.root.findByProps({ className: "message-list" });
      viewport.props.onPointerDown({ clientY: 200, pointerType: "touch" });
      windowListeners.get("pointermove")?.({ clientY: 201, pointerType: "touch" } as PointerEvent);
      viewport.props.onScroll({ currentTarget: messageList });
    });
    messageList.scrollHeight = 1500;
    act(() => tree.update(<Harness itemCount={2} />));

    expect(messageList.scrollTop).toBe(999);
  });

  it("does not infer touch intent while a resting finger overlaps content reflow", () => {
    const resize = installResizeObserver();
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
    act(() => viewport.props.onPointerDown({ clientY: 200, pointerType: "touch" }));
    messageList.scrollHeight = 800;
    messageList.scrollHeight = 2000;
    act(() => viewport.props.onScroll({ currentTarget: messageList }));
    act(() => resize.notify());

    expect(messageList.scrollTop).toBe(1600);
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
      viewport.props.onPointerDown({ clientX: 610, currentTarget: messageList, pointerType: "mouse" });
      viewport.props.onScroll({ currentTarget: messageList });
      viewport.props.onPointerUp({ pointerType: "mouse" });
    });
    messageList.scrollHeight = 1500;
    act(() => tree.update(<Harness itemCount={2} />));

    expect(messageList.scrollTop).toBe(700);
  });

  it("does not infer scrollbar intent from a pointer down in Chat content", () => {
    const resize = installResizeObserver();
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
      viewport.props.onPointerDown({ clientX: 200, currentTarget: messageList, pointerType: "mouse" });
      viewport.props.onScroll({ currentTarget: messageList });
      viewport.props.onPointerUp({ pointerType: "mouse" });
    });

    messageList.scrollHeight = 1500;
    act(() => resize.notify());
    expect(messageList.scrollTop).toBe(1100);
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
      viewport.props.onPointerDown({ clientX: 610, currentTarget: messageList, pointerType: "mouse" });
      windowListeners.get("pointerup")?.(new Event("pointerup"));
      tree.update(<Harness itemCount={1} />);
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

  it("excludes live row reflow from a synchronized-history prepend offset", () => {
    const resize = installResizeObserver();
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

    // A streamed row grows while native history is still being checked.
    messageList.scrollHeight = 1500;
    act(() => resize.notify());
    expect(messageList.scrollTop).toBe(700);

    // Only the 200px history prepend should move the reading viewport.
    messageList.scrollHeight = 1700;
    act(() => tree.update(<Harness historySyncState="updated" itemCount={2} />));
    expect(messageList.scrollTop).toBe(900);
  });

  it("does not treat live rows as prepended history when synchronization finishes idle", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<Harness historySyncState="syncing" itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });
    messageList.scrollTop = 700;
    const viewport = tree.root.findByProps({ className: "message-list" });
    act(() => {
      viewport.props.onWheel({ deltaY: -10 });
      viewport.props.onScroll({ currentTarget: messageList });
    });

    messageList.scrollHeight = 1600;
    act(() => tree.update(<Harness historySyncState="idle" itemCount={2} />));

    expect(messageList.scrollTop).toBe(700);
  });

  it("keeps the visible row anchored while an earlier page is prepended", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });
    act(() => {
      const viewport = tree.root.findByProps({ className: "message-list" });
      viewport.props.onWheel({ deltaY: -10 });
      messageList.scrollTop = 700;
      viewport.props.onScroll({ currentTarget: messageList });
      tree.root.findByProps({ className: "capture-prepend" }).props.onClick();
      tree.update(<Harness itemCount={1} pendingPrepend />);
    });

    messageList.scrollHeight = 1600;
    act(() => tree.update(<Harness itemCount={2} prependRequestGeneration={1} />));

    expect(messageList.scrollTop).toBe(900);
  });

  it("waits for the requested earlier page before consuming its prepend anchor", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });
    act(() => {
      const viewport = tree.root.findByProps({ className: "message-list" });
      viewport.props.onWheel({ deltaY: -10 });
      messageList.scrollTop = 700;
      viewport.props.onScroll({ currentTarget: messageList });
      tree.root.findByProps({ className: "capture-prepend" }).props.onClick();
      tree.update(<Harness itemCount={1} pendingPrepend />);
    });

    // A new live row arrives at the end of Chat while the earlier-page request is still pending.
    messageList.scrollHeight = 1500;
    act(() => tree.update(<Harness itemCount={2} pendingPrepend />));
    expect(messageList.scrollTop).toBe(700);

    // Only the requested page's prepend shifts the reader's visible row.
    messageList.scrollHeight = 1700;
    act(() => tree.update(<Harness itemCount={3} prependRequestGeneration={1} />));
    expect(messageList.scrollTop).toBe(900);
  });

  it("excludes intrinsic live-row growth from a pending earlier page's prepend offset", () => {
    const resize = installResizeObserver();
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });
    act(() => {
      const viewport = tree.root.findByProps({ className: "message-list" });
      viewport.props.onWheel({ deltaY: -10 });
      messageList.scrollTop = 700;
      viewport.props.onScroll({ currentTarget: messageList });
      tree.root.findByProps({ className: "capture-prepend" }).props.onClick();
      tree.update(<Harness itemCount={1} pendingPrepend />);
    });

    messageList.scrollHeight = 1500;
    act(() => resize.notify());

    messageList.scrollHeight = 1700;
    act(() => tree.update(<Harness itemCount={2} prependRequestGeneration={1} />));
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
  historySyncState = "idle",
  itemCount,
  pendingPrepend = false,
  prependRequestGeneration = pendingPrepend ? 1 : 0,
  taskId = "task_1",
}: {
  historySyncState?: "idle" | "checking" | "syncing" | "updated" | "failed";
  itemCount: number;
  pendingPrepend?: boolean;
  prependRequestGeneration?: number;
  taskId?: string;
}) {
  const [savedScrollStates, setSavedScrollStates] = useState<Record<string, TaskChatScrollState>>({
    task_1: { ownership: "following", scrollTop: 1000 },
  });
  const chatScroll = useTaskChatScroll({
    historySyncState,
    itemCount,
    onScrollState: (scrollState) => setSavedScrollStates((current) => ({
      ...current,
      [taskId]: scrollState,
    })),
    pendingPrepend,
    prependRequestGeneration,
    savedScrollState: savedScrollStates[taskId],
    taskId,
  });

  return (
    <>
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
      <button
        className="capture-prepend"
        onClick={() => chatScroll.capturePrependAnchor(prependRequestGeneration + 1)}
        type="button"
      />
      <button className="jump" onClick={chatScroll.jumpToLatest} type="button" />
    </>
  );
}

function scrollNode({ clientHeight, scrollHeight }: { clientHeight: number; scrollHeight: number }) {
  const children: Element[] = [];
  let currentScrollHeight = scrollHeight;
  let currentScrollTop = 0;
  return {
    clientHeight,
    clientWidth: 600,
    children,
    getBoundingClientRect: () => ({ left: 0, right: 616 }),
    offsetWidth: 616,
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

function installResizeObserver() {
  const observers: Array<{
    callback: ResizeObserverCallback;
    disconnect: ReturnType<typeof vi.fn>;
    observe: ReturnType<typeof vi.fn>;
    observer: ResizeObserver;
  }> = [];
  class MockResizeObserver implements ResizeObserver {
    readonly callback: ResizeObserverCallback;
    readonly disconnect = vi.fn();
    readonly observe = vi.fn();
    readonly unobserve = vi.fn();

    constructor(callback: ResizeObserverCallback) {
      this.callback = callback;
      observers.push({ callback, disconnect: this.disconnect, observe: this.observe, observer: this });
    }
  }
  vi.stubGlobal("ResizeObserver", MockResizeObserver);
  return {
    instances: observers,
    notify() {
      const latest = observers.at(-1);
      latest?.callback([], latest.observer);
    },
  };
}

function installMutationObserver() {
  const observers: Array<{
    callback: MutationCallback;
    disconnect: ReturnType<typeof vi.fn>;
    observer: MutationObserver;
  }> = [];
  class MockMutationObserver implements MutationObserver {
    readonly callback: MutationCallback;
    readonly disconnect = vi.fn();
    readonly observe = vi.fn();
    readonly takeRecords = vi.fn(() => []);

    constructor(callback: MutationCallback) {
      this.callback = callback;
      observers.push({ callback, disconnect: this.disconnect, observer: this });
    }
  }
  vi.stubGlobal("MutationObserver", MockMutationObserver);
  return {
    instances: observers,
    notify() {
      const latest = observers.at(-1);
      latest?.callback([], latest.observer);
    },
  };
}
