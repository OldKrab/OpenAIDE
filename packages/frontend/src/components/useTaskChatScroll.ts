import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent, PointerEvent, UIEvent, WheelEvent } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { TaskChatScrollState } from "../state/store";

type ScrollIntent = "towardEarlier" | "towardLatest";
type PointerScrollGesture =
  | { kind: "scrollbar" }
  | { kind: "touch"; lastClientY: number };
type PrependAnchor = {
  itemKeys: readonly string[];
  key?: string;
  scrollOffset: number;
  totalSize: number;
  viewportOffset?: number;
};

const SHOW_JUMP_TO_LATEST_DISTANCE_PX = 96;
const HIDE_JUMP_TO_LATEST_DISTANCE_PX = 48;
const OVERLAY_SCROLLBAR_HIT_WIDTH_PX = 10;
const AUTO_FILL_HISTORY_BUFFER_PX = 120;
const MAX_AUTO_FILL_PAGES = 4;
const CHAT_ROW_ESTIMATE_PX = 72;
const CHAT_ROW_GAP_PX = 8;
const CHAT_END_PADDING_PX = 64;
const CHAT_INITIAL_RECT = { width: 760, height: 600 };

type UseTaskChatScrollOptions = {
  beforeCursor?: string;
  hasEarlier: boolean;
  historySyncState?: "idle" | "syncing" | "updated";
  itemKeys: readonly string[];
  latestMessageKey?: string;
  onLoadEarlier: (beforeCursor: string) => number | undefined;
  onScrollState: (scrollState: TaskChatScrollState) => void;
  pendingPrepend: boolean;
  savedScrollState?: TaskChatScrollState;
  taskId: string;
};

/** Owns Chat reading intent while TanStack Virtual owns row geometry and anchoring. */
export function useTaskChatScroll(options: UseTaskChatScrollOptions) {
  const {
    beforeCursor,
    hasEarlier,
    historySyncState,
    itemKeys,
    latestMessageKey,
    onLoadEarlier,
    onScrollState,
    pendingPrepend,
    savedScrollState,
    taskId,
  } = options;
  const autoFillPageCountRef = useRef(0);
  const autoFillCursorRef = useRef<string | undefined>(undefined);
  const lastMessageKeyRef = useRef<string | undefined>(latestMessageKey);
  const lastScrollTopRef = useRef<number | undefined>(undefined);
  const messageListRef = useRef<HTMLDivElement | null>(null);
  const onScrollStateRef = useRef(onScrollState);
  const pointerGestureRef = useRef<PointerScrollGesture | undefined>(undefined);
  const prependAnchorRef = useRef<PrependAnchor | undefined>(undefined);
  const prependCorrectionFrameRef = useRef<number | undefined>(undefined);
  const scrollIntentRef = useRef<ScrollIntent | undefined>(undefined);
  const scrollOwnershipRef = useRef<TaskChatScrollState["ownership"]>(
    savedScrollState?.ownership ?? "following",
  );
  const [scrollOwnership, setScrollOwnershipState] = useState<TaskChatScrollState["ownership"]>(
    savedScrollState?.ownership ?? "following",
  );
  const [showJumpToLatest, setShowJumpToLatest] = useState(false);
  const [moreBelow, setMoreBelow] = useState(false);
  onScrollStateRef.current = onScrollState;

  const getItemKey = useCallback(
    (index: number) => itemKeys[index] ?? `missing-chat-row:${index}`,
    [itemKeys],
  );
  const virtualizer = useVirtualizer<HTMLDivElement, HTMLDivElement>({
    // Initial Chat must begin at its first row. Following later messages is
    // handled explicitly below, after the reader's intent is known.
    anchorTo: "start",
    count: itemKeys.length,
    estimateSize: () => CHAT_ROW_ESTIMATE_PX,
    followOnAppend: scrollOwnership === "following",
    gap: CHAT_ROW_GAP_PX,
    getItemKey,
    getScrollElement: () => messageListRef.current,
    // Avoid a blank first render before the App Shell viewport is measured.
    initialRect: CHAT_INITIAL_RECT,
    overscan: 6,
    paddingEnd: CHAT_END_PADDING_PX,
    scrollEndThreshold: 2,
    // React 19 can schedule these updates without forcing synchronous commits.
    useFlushSync: false,
  });

  const persistScrollState = useCallback((ownership = scrollOwnershipRef.current) => {
    const messageList = messageListRef.current;
    if (!messageList) return;
    onScrollStateRef.current({ ownership, scrollTop: messageList.scrollTop });
  }, []);

  // The persistent Load earlier control is the virtualizer's first row, so
  // its generic prepend anchor preserves that control. Product reading
  // continuity instead preserves the reader's position within the old range.
  useLayoutEffect(() => {
    const anchor = prependAnchorRef.current;
    if (!anchor) return;
    if (sameKeys(anchor.itemKeys, itemKeys)) return;
    const previousKeys = new Set(anchor.itemKeys);
    const insertedMessage = itemKeys.some(
      (key) => key.startsWith("message:") && !previousKeys.has(key),
    );
    if (!insertedMessage) {
      if (!pendingPrepend) prependAnchorRef.current = undefined;
      return;
    }
    prependAnchorRef.current = undefined;
    const prependedSize = virtualizer.getTotalSize() - anchor.totalSize;
    virtualizer.scrollToOffset(Math.max(0, anchor.scrollOffset + prependedSize));
    persistScrollState("reading");
    if (anchor.key && anchor.viewportOffset !== undefined && typeof requestAnimationFrame === "function") {
      const correctRetainedRow = (attempt: number) => {
        const messageList = messageListRef.current;
        if (!messageList) return;
        const row = [...messageList.querySelectorAll<HTMLElement>("[data-row-key]")]
          .find((candidate) => candidate.dataset.rowKey === anchor.key);
        if (row) {
          const viewportTop = messageList.getBoundingClientRect().top;
          const delta = row.getBoundingClientRect().top - viewportTop - anchor.viewportOffset!;
          if (Math.abs(delta) > 0.5) virtualizer.scrollBy(delta);
          if (Math.abs(delta) <= 0.5 || attempt >= 5) {
            persistScrollState("reading");
            prependCorrectionFrameRef.current = undefined;
            return;
          }
        }
        if (attempt >= 5) {
          prependCorrectionFrameRef.current = undefined;
          return;
        }
        prependCorrectionFrameRef.current = requestAnimationFrame(() => correctRetainedRow(attempt + 1));
      };
      prependCorrectionFrameRef.current = requestAnimationFrame(() => correctRetainedRow(0));
    }
  }, [itemKeys, pendingPrepend, persistScrollState, virtualizer]);

  const setScrollOwnership = useCallback((ownership: TaskChatScrollState["ownership"]) => {
    if (scrollOwnershipRef.current === ownership) return;
    scrollOwnershipRef.current = ownership;
    setScrollOwnershipState(ownership);
    persistScrollState(ownership);
  }, [persistScrollState]);

  const loadEarlier = useCallback((cursor: string) => {
    const messageList = messageListRef.current;
    const visibleAnchor = messageList ? firstVisibleMessageAnchor(messageList) : undefined;
    if (prependCorrectionFrameRef.current !== undefined && typeof cancelAnimationFrame === "function") {
      cancelAnimationFrame(prependCorrectionFrameRef.current);
      prependCorrectionFrameRef.current = undefined;
    }
    prependAnchorRef.current = messageList
      ? {
          itemKeys: [...itemKeys],
          ...visibleAnchor,
          scrollOffset: messageList.scrollTop,
          totalSize: virtualizer.getTotalSize(),
        }
      : undefined;
    // Paging backward is explicit reading intent even when a restored offset
    // reached the control without first producing a wheel/pointer gesture.
    setScrollOwnership("reading");
    const requestId = onLoadEarlier(cursor);
    if (requestId === undefined) prependAnchorRef.current = undefined;
    return requestId;
  }, [itemKeys, onLoadEarlier, setScrollOwnership, virtualizer]);

  useEffect(() => () => {
    if (prependCorrectionFrameRef.current !== undefined && typeof cancelAnimationFrame === "function") {
      cancelAnimationFrame(prependCorrectionFrameRef.current);
    }
  }, [taskId]);

  const setScrollIntent = useCallback((intent: ScrollIntent) => {
    if (scrollIntentRef.current === intent) return;
    scrollIntentRef.current = intent;
  }, []);

  const updateJumpToLatestVisibility = useCallback(() => {
    const distanceFromBottom = virtualizer.getDistanceFromEnd();
    setMoreBelow(distanceFromBottom > 2);
    setShowJumpToLatest((visible) => (
      visible
        ? distanceFromBottom > HIDE_JUMP_TO_LATEST_DISTANCE_PX
        : distanceFromBottom > SHOW_JUMP_TO_LATEST_DISTANCE_PX
    ));
  }, [virtualizer]);

  const autoLoadEarlierIfUnderfilled = useCallback(() => {
    const messageList = messageListRef.current;
    if (
      !messageList
      || !hasEarlier
      || !beforeCursor
      || pendingPrepend
      || messageList.clientHeight <= 0
      || virtualizer.getTotalSize() >= messageList.clientHeight + AUTO_FILL_HISTORY_BUFFER_PX
      || autoFillPageCountRef.current >= MAX_AUTO_FILL_PAGES
      || autoFillCursorRef.current === beforeCursor
    ) return;

    // One cursor gets one automatic attempt. Failure remains explicitly retryable.
    autoFillCursorRef.current = beforeCursor;
    autoFillPageCountRef.current += 1;
    onLoadEarlier(beforeCursor);
  }, [beforeCursor, hasEarlier, onLoadEarlier, pendingPrepend, virtualizer]);

  // Restore only when Task identity changes. The virtualizer keeps subsequent
  // prepends and size changes stable by keyed row identity.
  useLayoutEffect(() => {
    const ownership = savedScrollState?.ownership ?? "following";
    scrollOwnershipRef.current = ownership;
    setScrollOwnershipState(ownership);
    pointerGestureRef.current = undefined;
    scrollIntentRef.current = undefined;
    autoFillCursorRef.current = undefined;
    autoFillPageCountRef.current = 0;
    lastMessageKeyRef.current = latestMessageKey;

    if (ownership === "following") {
      virtualizer.scrollToEnd();
    } else {
      virtualizer.scrollToOffset(savedScrollState?.scrollTop ?? 0);
    }
    const messageList = messageListRef.current;
    lastScrollTopRef.current = messageList?.scrollTop;
    updateJumpToLatestVisibility();
  }, [taskId]);

  // A status/footer row can remain the final virtual row while a new Chat
  // message is inserted before it, so explicitly express the product's follow
  // intent through the virtualizer for that append shape.
  useLayoutEffect(() => {
    const previousMessageKey = lastMessageKeyRef.current;
    lastMessageKeyRef.current = latestMessageKey;
    if (
      previousMessageKey !== undefined
      && latestMessageKey !== undefined
      && latestMessageKey !== previousMessageKey
      && scrollOwnershipRef.current === "following"
    ) {
      virtualizer.scrollToEnd();
    }
  }, [latestMessageKey, virtualizer]);

  // A history replacement may not retain any keyed visible row. Followers
  // still land at latest; readers keep their current offset as the fallback.
  useLayoutEffect(() => {
    if (historySyncState === "updated" && scrollOwnershipRef.current === "following") {
      virtualizer.scrollToEnd();
    }
  }, [historySyncState, virtualizer]);

  // Panels above the composer change the Chat viewport height. Preserve the
  // current product ownership while delegating the actual move to TanStack.
  useLayoutEffect(() => {
    const messageList = messageListRef.current;
    if (!messageList || typeof ResizeObserver !== "function") return undefined;
    const observer = new ResizeObserver(() => {
      if (scrollOwnershipRef.current === "following") virtualizer.scrollToEnd();
      updateJumpToLatestVisibility();
      autoLoadEarlierIfUnderfilled();
    });
    observer.observe(messageList);
    return () => observer.disconnect();
  }, [autoLoadEarlierIfUnderfilled, taskId, updateJumpToLatestVisibility, virtualizer]);

  // ResizeObserver is present in every supported App Shell. Keep a render
  // fallback for tests and constrained embedded environments.
  useLayoutEffect(() => {
    if (typeof ResizeObserver === "function") return;
    if (scrollOwnershipRef.current === "following") virtualizer.scrollToEnd();
    updateJumpToLatestVisibility();
    autoLoadEarlierIfUnderfilled();
  });

  useLayoutEffect(() => {
    autoLoadEarlierIfUnderfilled();
  }, [autoLoadEarlierIfUnderfilled, itemKeys.length]);

  const finishPointerGesture = useCallback(() => {
    const gesture = pointerGestureRef.current;
    if (!gesture) return;
    pointerGestureRef.current = undefined;
    scrollIntentRef.current = undefined;
    if (gesture.kind === "scrollbar" && virtualizer.isAtEnd(2)) {
      setScrollOwnership("following");
    }
  }, [setScrollOwnership, virtualizer]);

  const trackTouchGesture = useCallback((event: globalThis.PointerEvent) => {
    const gesture = pointerGestureRef.current;
    if (gesture?.kind !== "touch" || event.pointerType !== "touch") return;
    const movement = event.clientY - gesture.lastClientY;
    if (movement === 0) return;
    gesture.lastClientY = event.clientY;
    if (movement > 0) {
      setScrollIntent("towardEarlier");
      setScrollOwnership("reading");
    } else {
      setScrollIntent("towardLatest");
    }
  }, [setScrollIntent, setScrollOwnership]);

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.addEventListener !== "function") return undefined;
    window.addEventListener("pointermove", trackTouchGesture);
    window.addEventListener("pointerup", finishPointerGesture);
    window.addEventListener("pointercancel", finishPointerGesture);
    return () => {
      window.removeEventListener("pointermove", trackTouchGesture);
      window.removeEventListener("pointerup", finishPointerGesture);
      window.removeEventListener("pointercancel", finishPointerGesture);
    };
  }, [finishPointerGesture, taskId, trackTouchGesture]);

  const onScroll = useCallback((event: UIEvent<HTMLDivElement>) => {
    const messageList = event.currentTarget;
    const previousScrollTop = lastScrollTopRef.current;
    const pointerGesture = pointerGestureRef.current;
    if (pointerGesture?.kind === "scrollbar" && previousScrollTop !== undefined) {
      if (messageList.scrollTop < previousScrollTop) {
        setScrollIntent("towardEarlier");
        setScrollOwnership("reading");
      } else if (messageList.scrollTop > previousScrollTop) {
        setScrollIntent("towardLatest");
      }
    }
    if (
      scrollOwnershipRef.current === "reading"
      && scrollIntentRef.current === "towardLatest"
      && virtualizer.isAtEnd(2)
    ) {
      setScrollOwnership("following");
    }
    scrollIntentRef.current = undefined;
    lastScrollTopRef.current = messageList.scrollTop;
    updateJumpToLatestVisibility();
    persistScrollState();
  }, [persistScrollState, setScrollIntent, setScrollOwnership, updateJumpToLatestVisibility, virtualizer]);

  const onWheel = useCallback((event: WheelEvent<HTMLDivElement>) => {
    if (event.deltaY === 0) return;
    if (event.deltaY < 0) {
      setScrollIntent("towardEarlier");
      setScrollOwnership("reading");
    } else {
      setScrollIntent("towardLatest");
      if (virtualizer.isAtEnd(2)) {
        scrollIntentRef.current = undefined;
        setScrollOwnership("following");
      }
    }
  }, [setScrollIntent, setScrollOwnership, virtualizer]);

  const onKeyDown = useCallback((event: KeyboardEvent<HTMLDivElement>) => {
    if (
      event.defaultPrevented
      || event.altKey
      || event.ctrlKey
      || event.metaKey
      || nestedControlOwnsScrollKey(event.target, event.currentTarget)
    ) return;
    const direction = keyboardScrollDirection(event.key, event.shiftKey);
    if (!direction) return;
    setScrollIntent(direction);
    if (direction === "towardEarlier") {
      setScrollOwnership("reading");
    } else if (virtualizer.isAtEnd(2)) {
      scrollIntentRef.current = undefined;
      setScrollOwnership("following");
    }
  }, [setScrollIntent, setScrollOwnership, virtualizer]);

  const onPointerDown = useCallback((event: PointerEvent<HTMLDivElement>) => {
    const gesture = event.pointerType === "touch"
      ? { kind: "touch" as const, lastClientY: event.clientY }
      : (isVerticalScrollbarPointer(event) ? { kind: "scrollbar" as const } : undefined);
    pointerGestureRef.current = gesture;
    if (gesture?.kind === "scrollbar") setScrollOwnership("reading");
  }, [setScrollOwnership]);

  const jumpToLatest = useCallback(() => {
    scrollIntentRef.current = undefined;
    setShowJumpToLatest(false);
    setScrollOwnership("following");
    virtualizer.scrollToEnd({ behavior: prefersReducedMotion() ? "auto" : "smooth" });
  }, [setScrollOwnership, virtualizer]);

  // Overlay expansion is reading intent: preserve the viewport before its scroll range changes.
  const pauseFollowing = useCallback(() => {
    setScrollOwnership("reading");
  }, [setScrollOwnership]);

  return useMemo(() => ({
    jumpToLatest,
    loadEarlier,
    messageListRef,
    moreBelow,
    onKeyDown,
    onPointerCancel: finishPointerGesture,
    onPointerDown,
    onPointerUp: finishPointerGesture,
    onScroll,
    onWheel,
    pauseFollowing,
    showJumpToLatest,
    virtualizer,
  }), [
    finishPointerGesture,
    jumpToLatest,
    loadEarlier,
    moreBelow,
    onKeyDown,
    onPointerDown,
    onScroll,
    onWheel,
    pauseFollowing,
    showJumpToLatest,
    virtualizer,
  ]);
}

function keyboardScrollDirection(key: string, shiftKey: boolean): ScrollIntent | undefined {
  if (key === "PageUp" || key === "Home" || key === "ArrowUp" || (key === " " && shiftKey)) {
    return "towardEarlier";
  }
  if (key === "PageDown" || key === "End" || key === "ArrowDown" || (key === " " && !shiftKey)) {
    return "towardLatest";
  }
  return undefined;
}

function firstVisibleMessageAnchor(messageList: HTMLDivElement) {
  const viewportTop = messageList.getBoundingClientRect().top;
  const row = [...messageList.querySelectorAll<HTMLElement>(
    '.message-list-virtual-row[data-row-kind="message"][data-row-key]',
  )].find((candidate) => candidate.getBoundingClientRect().bottom > viewportTop);
  const key = row?.dataset.rowKey;
  return row && key
    ? { key, viewportOffset: row.getBoundingClientRect().top - viewportTop }
    : undefined;
}

function sameKeys(left: readonly string[], right: readonly string[]) {
  return left.length === right.length && left.every((key, index) => key === right[index]);
}

function nestedControlOwnsScrollKey(target: EventTarget, viewport: HTMLDivElement) {
  if (target === viewport) return false;
  const closest = (target as { closest?: (selector: string) => Element | null }).closest;
  if (typeof closest !== "function") return true;
  return Boolean(closest.call(
    target,
    "a[href], button, input, select, summary, textarea, [contenteditable='true'], [role='listbox'], [role='slider']",
  ));
}

function isVerticalScrollbarPointer(event: PointerEvent<HTMLDivElement>) {
  if (event.pointerType !== "mouse") return false;
  if (event.currentTarget.scrollHeight <= event.currentTarget.clientHeight) return false;
  const scrollbarWidth = Math.max(
    event.currentTarget.offsetWidth - event.currentTarget.clientWidth,
    OVERLAY_SCROLLBAR_HIT_WIDTH_PX,
  );
  const bounds = event.currentTarget.getBoundingClientRect();
  return event.clientX >= bounds.right - scrollbarWidth && event.clientX <= bounds.right;
}

function prefersReducedMotion() {
  return typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}
