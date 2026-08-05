import { useState } from "react";
import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskChatScrollState } from "../state/store";

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
      getOffsetForIndex: (index: number) => [index * 72, "start"] as const,
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
      scrollToEnd: () => {
        const viewport = element();
        if (viewport) viewport.scrollTop = Math.max(0, viewport.scrollHeight - viewport.clientHeight);
      },
      scrollToOffset: (offset: number) => {
        const viewport = element();
        if (viewport) viewport.scrollTop = offset;
      },
      scrollBy: (delta: number) => {
        const viewport = element();
        if (viewport) viewport.scrollTop += delta;
      },
    };
  }
});

import { useTaskChatScroll } from "./useTaskChatScroll";

describe("useTaskChatScroll", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("keeps a following viewport pinned when mounted Chat content changes size", () => {
    const resize = installResizeObserver();
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    renderHarness(messageList);
    expect(messageList.scrollTop).toBe(1000);

    messageList.scrollHeight = 2200;
    act(() => resize.notify());

    expect(messageList.scrollTop).toBe(1800);
  });

  it("leaves follow mode without moving the viewport when an overlay expands", () => {
    const resize = installResizeObserver();
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    const onScrollState = vi.fn();
    const tree = renderHarness(messageList, { onScrollState });
    expect(messageList.scrollTop).toBe(1000);

    act(() => tree.root.findByProps({ className: "pause-follow-test" }).props.onClick());
    messageList.scrollHeight = 1800;
    act(() => resize.notify());

    expect(messageList.scrollTop).toBe(1000);
    expect(onScrollState).toHaveBeenLastCalledWith({ ownership: "reading", scrollTop: 1000 });
  });

  it("does not pull a reader down, then resumes following after explicit downward intent", () => {
    const resize = installResizeObserver();
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    const tree = renderHarness(messageList);
    const viewport = messageListView(tree);

    act(() => {
      viewport.props.onWheel({ deltaY: -8 });
      messageList.scrollTop = 700;
      viewport.props.onScroll({ currentTarget: messageList });
    });
    messageList.scrollHeight = 1600;
    act(() => resize.notify());
    expect(messageList.scrollTop).toBe(700);

    act(() => {
      viewport.props.onWheel({ deltaY: 8 });
      messageList.scrollTop = 1200;
      viewport.props.onScroll({ currentTarget: messageList });
    });
    messageList.scrollHeight = 1800;
    act(() => resize.notify());
    expect(messageList.scrollTop).toBe(1400);
  });

  it("claims reader ownership from upward keyboard navigation but not from a nested control", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    const onScrollState = vi.fn();
    const tree = renderHarness(messageList, { onScrollState });
    const viewport = messageListView(tree);

    act(() => viewport.props.onKeyDown({
      altKey: false,
      ctrlKey: false,
      currentTarget: messageList,
      defaultPrevented: false,
      key: "PageUp",
      metaKey: false,
      shiftKey: false,
      target: messageList,
    }));
    expect(onScrollState).toHaveBeenLastCalledWith({ ownership: "reading", scrollTop: 1000 });

    const nestedButton = { closest: () => ({}) };
    act(() => viewport.props.onKeyDown({
      altKey: false,
      ctrlKey: false,
      currentTarget: messageList,
      defaultPrevented: false,
      key: "PageUp",
      metaKey: false,
      shiftKey: false,
      target: nestedButton,
    }));
    expect(onScrollState).toHaveBeenCalledTimes(1);
  });

  it("restores a saved reader offset when switching Tasks", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1800 });
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(
        <Harness
          onScrollState={vi.fn()}
          savedScrollState={{ ownership: "reading", scrollTop: 320 }}
          taskId="task_2"
        />,
        { createNodeMock: () => messageList },
      );
    });

    expect(messageList.scrollTop).toBe(320);
    expect(messageListView(tree)).toBeTruthy();
  });

  it("automatically requests earlier history only while estimated Chat is underfilled", () => {
    const shortList = scrollNode({ clientHeight: 400, scrollHeight: 180 });
    const onLoadEarlier = vi.fn(() => 1);
    renderHarness(shortList, { beforeCursor: "cursor-1", hasEarlier: true, onLoadEarlier });
    expect(onLoadEarlier).toHaveBeenCalledWith("cursor-1");

    const filledList = scrollNode({ clientHeight: 400, scrollHeight: 900 });
    const filledLoad = vi.fn(() => 1);
    renderHarness(filledList, { beforeCursor: "cursor-2", hasEarlier: true, onLoadEarlier: filledLoad });
    expect(filledLoad).not.toHaveBeenCalled();
  });

  it("keeps the reader at the same position within retained content after a prepend", () => {
    const animationFrames = installAnimationFrames();
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1200 });
    let retainedContentTop = 100;
    const retainedRow = {
      dataset: { rowKey: "message:retained" },
      getBoundingClientRect: () => ({
        bottom: retainedContentTop - messageList.scrollTop + 72,
        top: retainedContentTop - messageList.scrollTop,
      }),
    };
    messageList.querySelectorAll = () => [retainedRow] as unknown as never[];
    const onLoadEarlier = vi.fn(() => 1);
    const onScrollState = vi.fn();
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(
        <Harness
          itemKeys={["load-earlier", "message:retained"]}
          onLoadEarlier={onLoadEarlier}
          onScrollState={onScrollState}
        />,
        { createNodeMock: () => messageList },
      );
    });
    messageList.scrollTop = 0;

    act(() => tree.root.findByProps({ className: "load-earlier-test" }).props.onClick());
    expect(onLoadEarlier).toHaveBeenCalledWith("cursor-before");
    expect(onScrollState).toHaveBeenLastCalledWith({ ownership: "reading", scrollTop: 0 });

    act(() => {
      messageList.scrollHeight = 1344;
      retainedContentTop = 499;
      tree.update(
        <Harness
          itemKeys={[
            "load-earlier",
            "message:older-1",
            "message:older-2",
            "message:retained",
          ]}
          onLoadEarlier={onLoadEarlier}
          onScrollState={onScrollState}
        />,
      );
    });

    expect(messageList.scrollTop).toBe(144);
    expect(animationFrames.pending()).toBeGreaterThan(0);
    act(() => animationFrames.flush());
    expect(messageList.scrollTop).toBe(399);
    expect(retainedRow.getBoundingClientRect().top).toBe(100);
  });
});

function Harness({
  beforeCursor,
  hasEarlier = false,
  itemKeys = ["message:1", "timeline-status"],
  onLoadEarlier = () => undefined,
  onScrollState,
  savedScrollState,
  taskId = "task_1",
}: {
  beforeCursor?: string;
  hasEarlier?: boolean;
  itemKeys?: string[];
  onLoadEarlier?: (cursor: string) => number | undefined;
  onScrollState?: (state: TaskChatScrollState) => void;
  savedScrollState?: TaskChatScrollState;
  taskId?: string;
}) {
  const [savedState, setSavedState] = useState(savedScrollState);
  const chatScroll = useTaskChatScroll({
    beforeCursor,
    hasEarlier,
    itemKeys,
    latestMessageKey: "message:1",
    onLoadEarlier,
    onScrollState: (state) => {
      setSavedState(state);
      onScrollState?.(state);
    },
    pendingPrepend: false,
    savedScrollState: savedState,
    taskId,
  });
  return (
    <>
      <div
        className="message-list"
        onKeyDown={chatScroll.onKeyDown}
        onPointerCancel={chatScroll.onPointerCancel}
        onPointerDown={chatScroll.onPointerDown}
        onPointerUp={chatScroll.onPointerUp}
        onScroll={chatScroll.onScroll}
        onWheel={chatScroll.onWheel}
        ref={chatScroll.messageListRef}
      />
      <button className="load-earlier-test" onClick={() => chatScroll.loadEarlier("cursor-before")} />
      <button className="pause-follow-test" onClick={chatScroll.pauseFollowing} />
    </>
  );
}

function renderHarness(
  messageList: ReturnType<typeof scrollNode>,
  props: React.ComponentProps<typeof Harness> = {},
) {
  let tree!: ReactTestRenderer;
  act(() => {
    tree = create(<Harness {...props} />, { createNodeMock: () => messageList });
  });
  return tree;
}

function messageListView(tree: ReactTestRenderer) {
  return tree.root.findByProps({ className: "message-list" });
}

function scrollNode({ clientHeight, scrollHeight }: { clientHeight: number; scrollHeight: number }) {
  let currentScrollHeight = scrollHeight;
  let currentScrollTop = 0;
  return {
    clientHeight,
    clientWidth: 600,
    getBoundingClientRect: () => ({ bottom: clientHeight, height: clientHeight, left: 0, right: 616, top: 0, width: 600 }),
    offsetWidth: 616,
    querySelectorAll: () => [],
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
  const observers: Array<{ callback: ResizeObserverCallback; observer: ResizeObserver }> = [];
  class MockResizeObserver implements ResizeObserver {
    readonly disconnect = vi.fn();
    readonly observe = vi.fn();
    readonly unobserve = vi.fn();

    constructor(readonly callback: ResizeObserverCallback) {
      observers.push({ callback, observer: this });
    }
  }
  vi.stubGlobal("ResizeObserver", MockResizeObserver);
  return {
    notify() {
      for (const { callback, observer } of observers) callback([], observer);
    },
  };
}

function installAnimationFrames() {
  let nextId = 1;
  const callbacks = new Map<number, FrameRequestCallback>();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    const id = nextId;
    nextId += 1;
    callbacks.set(id, callback);
    return id;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => callbacks.delete(id));
  return {
    pending: () => callbacks.size,
    flush() {
      for (let attempt = 0; callbacks.size > 0 && attempt < 10; attempt += 1) {
        const frame = [...callbacks.entries()];
        callbacks.clear();
        for (const [, callback] of frame) callback(performance.now());
      }
    },
  };
}
