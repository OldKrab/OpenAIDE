import { useState } from "react";
import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskChatScrollState } from "../state/store";

const virtualizerOptions = vi.hoisted(() => ({
  latest: undefined as { anchorTo?: "end" | "start" } | undefined,
  scrollToIndex: vi.fn(),
}));

vi.mock("@tanstack/react-virtual", async () => {
  const React = await import("react");
  type Options = {
    anchorTo?: "end" | "start";
    count: number;
    getItemKey: (index: number) => string;
    getScrollElement: () => HTMLDivElement | null;
  };
  return {
    useVirtualizer: (options: Options) => {
      virtualizerOptions.latest = options;
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
      scrollToIndex: (index: number, options?: { align?: string; behavior?: ScrollBehavior }) => {
        virtualizerOptions.scrollToIndex(index, options);
        const viewport = element();
        if (viewport) viewport.scrollTop = index * 72;
      },
      scrollBy: (delta: number) => {
        const viewport = element();
        if (viewport) viewport.scrollTop += delta;
      },
    };
  }
});

import { useTaskChatScroll } from "./useTaskChatScroll";
import type { UserMessageAnchor } from "./useTaskChatScroll";

describe("useTaskChatScroll", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    virtualizerOptions.latest = undefined;
    virtualizerOptions.scrollToIndex.mockClear();
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

  it("starts Chat from its first row instead of an end anchor", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 72 });

    renderHarness(messageList, { itemKeys: ["message:first"] });

    expect(virtualizerOptions.latest?.anchorTo).toBe("start");
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

  it("smoothly navigates between loaded User messages and announces the destination", () => {
    vi.useFakeTimers();
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    const onScrollState = vi.fn();
    const tree = renderHarness(messageList, {
      itemKeys: [
        "message:user-1",
        "message:agent-1",
        "message:agent-2",
        "message:agent-3",
        "message:agent-4",
        "message:agent-5",
        "message:agent-6",
        "message:agent-7",
        "message:user-2",
      ],
      onScrollState,
      savedScrollState: { ownership: "reading", scrollTop: 0 },
      userMessageAnchors: [
        { key: "message:user-1", rowIndex: 0, text: "First question" },
        { key: "message:user-2", rowIndex: 8, text: "Second question" },
      ],
    });

    act(() => tree.root.findByProps({ className: "next-user-message-test" }).props.onClick());

    expect(virtualizerOptions.scrollToIndex).toHaveBeenCalledWith(8, {
      align: "start",
      behavior: "smooth",
    });
    expect(tree.root.findByProps({ className: "user-message-navigation-state" }).props).toMatchObject({
      "data-announcement": "User message 2 of 2.",
      "data-current-index": 1,
      "data-target-key": "message:user-2",
    });
    act(() => messageListView(tree).props.onScroll({ currentTarget: messageList }));
    expect(onScrollState).not.toHaveBeenCalled();
    act(() => {
      vi.runAllTimers();
    });
    expect(onScrollState).toHaveBeenLastCalledWith({ ownership: "reading", scrollTop: 576 });
    act(() => tree.unmount());
    vi.useRealTimers();
  });

  it("selects a visible User message near the viewport center over an invisible predecessor", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1000 });
    const tree = renderHarness(messageList, {
      itemKeys: [
        "message:user-1",
        "message:agent-1",
        "message:agent-2",
        "message:agent-3",
        "message:user-2",
      ],
      savedScrollState: { ownership: "reading", scrollTop: 100 },
      userMessageAnchors: [
        { key: "message:user-1", rowIndex: 0, text: "Invisible preceding question" },
        { key: "message:user-2", rowIndex: 4, text: "Visible centered question" },
      ],
    });

    expect(tree.root.findByProps({ className: "user-message-navigation-state" }).props)
      .toMatchObject({ "data-current-index": 1 });
    act(() => tree.unmount());
  });

  it("keeps sequential navigation relative to the selected message while smooth scrolling", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    const tree = renderHarness(messageList, {
      itemKeys: [
        "message:user-1",
        "message:agent-1",
        "message:user-2",
        "message:agent-2",
        "message:user-3",
      ],
      userMessageAnchors: [
        { key: "message:user-1", rowIndex: 0, text: "First question" },
        { key: "message:user-2", rowIndex: 2, text: "Second question" },
        { key: "message:user-3", rowIndex: 4, text: "Third question" },
      ],
    });
    const previous = tree.root.findByProps({ className: "previous-user-message-test" });

    act(() => previous.props.onClick());
    act(() => {
      messageList.scrollTop = 1000;
      messageListView(tree).props.onScroll({ currentTarget: messageList });
    });
    act(() => previous.props.onClick());

    expect(virtualizerOptions.scrollToIndex.mock.calls.map(([index]) => index)).toEqual([2, 0]);
    act(() => tree.unmount());
  });

  it("loads one earlier page at the boundary and continues to the nearest new User message", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    const onLoadEarlier = vi.fn(() => 41);
    let tree!: ReactTestRenderer;
    const initialProps: React.ComponentProps<typeof Harness> = {
      beforeCursor: "cursor-before",
      hasEarlier: true,
      itemKeys: ["load-earlier", "message:retained"],
      onLoadEarlier,
      pendingPrepend: false,
      savedScrollState: { ownership: "reading", scrollTop: 0 },
      userMessageAnchors: [
        { key: "message:retained", rowIndex: 1, text: "Retained question" },
      ],
    };
    act(() => {
      tree = create(<Harness {...initialProps} />, { createNodeMock: () => messageList });
    });

    act(() => tree.root.findByProps({ className: "previous-user-message-test" }).props.onClick());
    expect(onLoadEarlier).toHaveBeenCalledTimes(1);
    expect(onLoadEarlier).toHaveBeenCalledWith("cursor-before");

    act(() => tree.update(<Harness {...initialProps} pendingPrepend />));
    act(() => tree.update(
      <Harness
        {...initialProps}
        itemKeys={["load-earlier", "message:older", "message:retained"]}
        pendingPrepend={false}
        userMessageAnchors={[
          { key: "message:older", rowIndex: 1, text: "Newly loaded question" },
          { key: "message:retained", rowIndex: 2, text: "Retained question" },
        ]}
      />,
    ));

    expect(virtualizerOptions.scrollToIndex).toHaveBeenLastCalledWith(1, {
      align: "start",
      behavior: "smooth",
    });
    expect(onLoadEarlier).toHaveBeenCalledTimes(1);
    expect(tree.root.findByProps({ className: "user-message-navigation-state" }).props).toMatchObject({
      "data-current-index": 0,
      "data-target-key": "message:older",
    });
    act(() => tree.unmount());
  });
});

function Harness({
  beforeCursor,
  hasEarlier = false,
  itemKeys = ["message:1", "timeline-status"],
  onLoadEarlier = () => undefined,
  onScrollState,
  pendingPrepend = false,
  savedScrollState,
  taskId = "task_1",
  userMessageAnchors = [],
}: {
  beforeCursor?: string;
  hasEarlier?: boolean;
  itemKeys?: string[];
  onLoadEarlier?: (cursor: string) => number | undefined;
  onScrollState?: (state: TaskChatScrollState) => void;
  pendingPrepend?: boolean;
  savedScrollState?: TaskChatScrollState;
  taskId?: string;
  userMessageAnchors?: UserMessageAnchor[];
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
    pendingPrepend,
    savedScrollState: savedState,
    taskId,
    userMessageAnchors,
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
      <button className="previous-user-message-test" onClick={chatScroll.userMessageNavigation.goPrevious} />
      <button className="next-user-message-test" onClick={chatScroll.userMessageNavigation.goNext} />
      <output
        className="user-message-navigation-state"
        data-announcement={chatScroll.userMessageNavigation.announcement}
        data-current-index={chatScroll.userMessageNavigation.currentIndex}
        data-pending={chatScroll.userMessageNavigation.pendingPrevious}
        data-target-key={chatScroll.userMessageNavigation.targetKey}
      />
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
