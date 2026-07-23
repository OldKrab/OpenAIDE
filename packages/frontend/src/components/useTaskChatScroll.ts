import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent, PointerEvent, UIEvent, WheelEvent } from "react";
import {
  scrollTopAfterPrependedContent,
  scrollTopForFollowingViewport,
} from "./TaskViewModel";
import type { TaskChatScrollState } from "../state/store";

type ScrollIntent = "towardEarlier" | "towardLatest";
type PointerScrollGesture =
  | { kind: "scrollbar" }
  | { kind: "touch"; lastClientY: number };
type ScrollAnchor = { scrollHeight: number; scrollTop: number };
type HistoryAnchor = ScrollAnchor & { ownership: TaskChatScrollState["ownership"] };
type PrependAnchor = ScrollAnchor & { requestGeneration: number; requestStarted: boolean };

const SHOW_JUMP_TO_LATEST_DISTANCE_PX = 96;
const HIDE_JUMP_TO_LATEST_DISTANCE_PX = 48;
const JUMP_TO_LATEST_DURATION_MS = 180;
const OVERLAY_SCROLLBAR_HIT_WIDTH_PX = 10;

type UseTaskChatScrollOptions = {
  historySyncState?: "idle" | "syncing" | "updated";
  itemCount: number;
  onScrollState: (scrollState: TaskChatScrollState) => void;
  pendingPrepend: boolean;
  prependRequestGeneration: number;
  savedScrollState?: TaskChatScrollState;
  taskId: string;
};

/** Owns the Chat viewport: content follows only until explicit reader input takes control. */
export function useTaskChatScroll(options: UseTaskChatScrollOptions) {
  const {
    historySyncState,
    itemCount,
    onScrollState,
    pendingPrepend,
    prependRequestGeneration,
    savedScrollState,
    taskId,
  } = options;
  const messageListRef = useRef<HTMLDivElement | null>(null);
  const historyAnchorRef = useRef<HistoryAnchor | undefined>(undefined);
  const jumpAnimationFrameRef = useRef<number | undefined>(undefined);
  const lastScrollTopRef = useRef<number | undefined>(undefined);
  const onScrollStateRef = useRef(onScrollState);
  const pointerGestureRef = useRef<PointerScrollGesture | undefined>(undefined);
  const prependAnchorRef = useRef<PrependAnchor | undefined>(undefined);
  const scrollIntentRef = useRef<ScrollIntent | undefined>(undefined);
  const scrollOwnershipRef = useRef<TaskChatScrollState["ownership"]>("following");
  const [showJumpToLatest, setShowJumpToLatest] = useState(false);
  const [moreBelow, setMoreBelow] = useState(false);
  onScrollStateRef.current = onScrollState;

  const setScrollOwnership = useCallback((ownership: TaskChatScrollState["ownership"]) => {
    if (scrollOwnershipRef.current === ownership) return;
    scrollOwnershipRef.current = ownership;
    const messageList = messageListRef.current;
    if (messageList) onScrollStateRef.current({ ownership, scrollTop: messageList.scrollTop });
  }, []);

  const setScrollIntent = useCallback((intent: ScrollIntent) => {
    if (scrollIntentRef.current === intent) return;
    scrollIntentRef.current = intent;
  }, []);

  const updateJumpToLatestVisibility = useCallback((messageList: HTMLDivElement) => {
    const distanceFromBottom = distanceFromLatest(messageList);
    setMoreBelow(distanceFromBottom > 2);
    setShowJumpToLatest((visible) => (
      visible
        ? distanceFromBottom > HIDE_JUMP_TO_LATEST_DISTANCE_PX
        : distanceFromBottom > SHOW_JUMP_TO_LATEST_DISTANCE_PX
    ));
  }, []);

  const cancelJumpAnimation = useCallback(() => {
    const frame = jumpAnimationFrameRef.current;
    if (frame === undefined) return;
    globalThis.cancelAnimationFrame?.(frame);
    jumpAnimationFrameRef.current = undefined;
  }, []);

  const reconcileViewport = useCallback((messageList: HTMLDivElement) => {
    const followingScrollTop = scrollTopForFollowingViewport({
      clientHeight: messageList.clientHeight,
      ownership: scrollOwnershipRef.current,
      scrollHeight: messageList.scrollHeight,
    });
    if (followingScrollTop !== undefined) messageList.scrollTop = followingScrollTop;
    lastScrollTopRef.current = messageList.scrollTop;
    updateJumpToLatestVisibility(messageList);
  }, [updateJumpToLatestVisibility]);

  const refreshPendingPrependBaseline = useCallback((messageList: HTMLDivElement) => {
    const anchor = prependAnchorRef.current;
    if (
      !anchor
      || !pendingPrepend
      || prependRequestGeneration !== anchor.requestGeneration
    ) return false;
    anchor.requestStarted = true;
    anchor.scrollHeight = messageList.scrollHeight;
    anchor.scrollTop = messageList.scrollTop;
    return true;
  }, [pendingPrepend, prependRequestGeneration]);

  const refreshHistorySyncBaseline = useCallback((messageList: HTMLDivElement) => {
    const anchor = historyAnchorRef.current;
    if (!anchor) return false;
    // Intrinsic growth while history is syncing belongs to the live
    // timeline, not the later native-history prepend.
    anchor.scrollHeight = messageList.scrollHeight;
    anchor.scrollTop = messageList.scrollTop;
    anchor.ownership = scrollOwnershipRef.current;
    return true;
  }, []);

  // Scroll persistence feeds this hook's props. Restore only when Task identity changes.
  useLayoutEffect(() => {
    const messageList = messageListRef.current;
    if (!messageList) return undefined;
    cancelJumpAnimation();
    historyAnchorRef.current = undefined;
    pointerGestureRef.current = undefined;
    prependAnchorRef.current = undefined;
    scrollIntentRef.current = undefined;

    scrollOwnershipRef.current = savedScrollState?.ownership ?? "following";
    messageList.scrollTop = savedScrollState?.scrollTop ?? messageList.scrollHeight;
    reconcileViewport(messageList);

    return () => {
      cancelJumpAnimation();
      historyAnchorRef.current = undefined;
      pointerGestureRef.current = undefined;
      prependAnchorRef.current = undefined;
      scrollIntentRef.current = undefined;
    };
  }, [cancelJumpAnimation, reconcileViewport, taskId]);

  const finishPointerGesture = useCallback(() => {
    const gesture = pointerGestureRef.current;
    if (!gesture) return;
    pointerGestureRef.current = undefined;
    scrollIntentRef.current = undefined;
    const messageList = messageListRef.current;
    if (gesture.kind === "scrollbar" && messageList && isAtLatest(messageList)) {
      setScrollOwnership("following");
    }
  }, [setScrollOwnership]);

  const trackTouchGesture = useCallback((event: globalThis.PointerEvent) => {
    const gesture = pointerGestureRef.current;
    if (gesture?.kind !== "touch" || event.pointerType !== "touch") return;
    const movement = event.clientY - gesture.lastClientY;
    if (movement === 0) return;
    gesture.lastClientY = event.clientY;
    cancelJumpAnimation();
    if (movement > 0) {
      setScrollIntent("towardEarlier");
      setScrollOwnership("reading");
    } else {
      setScrollIntent("towardLatest");
    }
  }, [cancelJumpAnimation, setScrollIntent, setScrollOwnership]);

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

  // Rows are observed directly because an overflow container's own box does not change when
  // markdown, images, permissions, or tool details change its scroll height.
  useLayoutEffect(() => {
    const messageList = messageListRef.current;
    if (!messageList || typeof ResizeObserver !== "function") return undefined;
    let active = true;
    const onResize = () => {
      if (!active || messageListRef.current !== messageList) return;
      reconcileViewport(messageList);
      refreshHistorySyncBaseline(messageList);
      refreshPendingPrependBaseline(messageList);
    };
    const resizeObserver = new ResizeObserver(onResize);
    const observedChildren = new Set<Element>();
    const observeCurrentChildren = () => {
      const currentChildren = new Set(Array.from(messageList.children));
      for (const child of observedChildren) {
        if (currentChildren.has(child)) continue;
        resizeObserver.unobserve(child);
        observedChildren.delete(child);
      }
      for (const child of currentChildren) {
        if (observedChildren.has(child)) continue;
        resizeObserver.observe(child);
        observedChildren.add(child);
      }
    };
    resizeObserver.observe(messageList);
    observeCurrentChildren();
    const mutationObserver = typeof MutationObserver === "function"
      ? new MutationObserver(() => {
          observeCurrentChildren();
          onResize();
        })
      : undefined;
    // Direct row changes are enough to refresh observation. Intrinsic changes
    // inside a row are reported by that row's ResizeObserver without walking
    // every Markdown mutation in the full Chat subtree.
    mutationObserver?.observe(messageList, { childList: true });

    return () => {
      active = false;
      mutationObserver?.disconnect();
      resizeObserver.disconnect();
    };
  }, [reconcileViewport, refreshHistorySyncBaseline, refreshPendingPrependBaseline, taskId]);

  useLayoutEffect(() => {
    const messageList = messageListRef.current;
    if (!messageList) return;
    if (historySyncState === "syncing") {
      historyAnchorRef.current ??= {
        scrollHeight: messageList.scrollHeight,
        scrollTop: messageList.scrollTop,
        ownership: scrollOwnershipRef.current,
      };
      return;
    }
    const anchor = historyAnchorRef.current;
    if (!anchor) return;
    if (historySyncState === "updated") {
      messageList.scrollTop = anchor.ownership === "following"
        ? latestScrollTop(messageList)
        : scrollTopAfterPrependedContent({
            previousScrollHeight: anchor.scrollHeight,
            previousScrollTop: anchor.scrollTop,
            nextScrollHeight: messageList.scrollHeight,
          });
    }
    historyAnchorRef.current = undefined;
    reconcileViewport(messageList);
  }, [historySyncState, reconcileViewport]);

  useLayoutEffect(() => {
    const messageList = messageListRef.current;
    const anchor = prependAnchorRef.current;
    if (!messageList || !anchor) return;
    if (prependRequestGeneration < anchor.requestGeneration) return;
    if (prependRequestGeneration > anchor.requestGeneration) {
      prependAnchorRef.current = undefined;
      return;
    }
    if (refreshPendingPrependBaseline(messageList)) {
      // While the request is in flight, new live rows and intrinsic layout changes are
      // unrelated to the future prepend. Advance the baseline without moving the reader.
      return;
    }
    if (!anchor.requestStarted) return;
    messageList.scrollTop = scrollTopAfterPrependedContent({
      previousScrollHeight: anchor.scrollHeight,
      previousScrollTop: anchor.scrollTop,
      nextScrollHeight: messageList.scrollHeight,
    });
    prependAnchorRef.current = undefined;
    reconcileViewport(messageList);
  }, [itemCount, pendingPrepend, prependRequestGeneration, reconcileViewport, refreshPendingPrependBaseline]);

  // ResizeObserver owns browser layout reconciliation. The render fallback is
  // retained only for test/legacy environments that do not provide it.
  useLayoutEffect(() => {
    if (typeof ResizeObserver === "function") return;
    const messageList = messageListRef.current;
    if (messageList) reconcileViewport(messageList);
  });

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
      && isAtLatest(messageList)
    ) {
      scrollIntentRef.current = undefined;
      setScrollOwnership("following");
    }
    // Wheel/keyboard intent belongs to this scroll transaction; pointer gestures set it again
    // for each movement while they remain active.
    scrollIntentRef.current = undefined;
    reconcileViewport(messageList);
    if (historyAnchorRef.current) {
      historyAnchorRef.current = {
        scrollHeight: messageList.scrollHeight,
        scrollTop: messageList.scrollTop,
        ownership: scrollOwnershipRef.current,
      };
    }
    onScrollStateRef.current({ ownership: scrollOwnershipRef.current, scrollTop: messageList.scrollTop });
  }, [reconcileViewport, setScrollIntent, setScrollOwnership]);

  const onWheel = useCallback((event: WheelEvent<HTMLDivElement>) => {
    if (event.deltaY === 0) return;
    cancelJumpAnimation();
    if (event.deltaY < 0) {
      setScrollIntent("towardEarlier");
      setScrollOwnership("reading");
    } else {
      setScrollIntent("towardLatest");
      const messageList = event.currentTarget ?? messageListRef.current;
      if (messageList && isAtLatest(messageList)) {
        scrollIntentRef.current = undefined;
        setScrollOwnership("following");
      }
    }
  }, [cancelJumpAnimation, setScrollIntent, setScrollOwnership]);

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
    cancelJumpAnimation();
    setScrollIntent(direction);
    if (direction === "towardEarlier") {
      setScrollOwnership("reading");
    } else if (isAtLatest(event.currentTarget)) {
      scrollIntentRef.current = undefined;
      setScrollOwnership("following");
    }
  }, [cancelJumpAnimation, setScrollIntent, setScrollOwnership]);

  const onPointerDown = useCallback((event: PointerEvent<HTMLDivElement>) => {
    const gesture = event.pointerType === "touch"
      ? { kind: "touch" as const, lastClientY: event.clientY }
      : (isVerticalScrollbarPointer(event) ? { kind: "scrollbar" as const } : undefined);
    pointerGestureRef.current = gesture;
    if (gesture?.kind !== "scrollbar") return;
    // Claim reader ownership before the browser's first drag scroll event so live
    // content reflow cannot reconcile the viewport back to Follow mode.
    cancelJumpAnimation();
    setScrollOwnership("reading");
  }, [cancelJumpAnimation, setScrollOwnership]);

  const capturePrependAnchor = useCallback((requestGeneration: number) => {
    const messageList = messageListRef.current;
    if (!messageList) return;
    prependAnchorRef.current = {
      requestGeneration,
      requestStarted: false,
      scrollHeight: messageList.scrollHeight,
      scrollTop: messageList.scrollTop,
    };
  }, []);

  const jumpToLatest = useCallback(() => {
    const messageList = messageListRef.current;
    if (!messageList) return;
    cancelJumpAnimation();
    scrollIntentRef.current = undefined;
    setShowJumpToLatest(false);
    const finish = () => {
      messageList.scrollTop = latestScrollTop(messageList);
      lastScrollTopRef.current = messageList.scrollTop;
      scrollOwnershipRef.current = "following";
      onScrollStateRef.current({ ownership: "following", scrollTop: messageList.scrollTop });
    };
    if (prefersReducedMotion() || typeof requestAnimationFrame !== "function") {
      finish();
      return;
    }
    const startScrollTop = messageList.scrollTop;
    const startedAt = performance.now();
    const animate = (now: number) => {
      const progress = Math.min(1, (now - startedAt) / JUMP_TO_LATEST_DURATION_MS);
      const easedProgress = 1 - ((1 - progress) ** 4);
      const targetScrollTop = latestScrollTop(messageList);
      messageList.scrollTop = startScrollTop + ((targetScrollTop - startScrollTop) * easedProgress);
      lastScrollTopRef.current = messageList.scrollTop;
      if (progress < 1) {
        jumpAnimationFrameRef.current = requestAnimationFrame(animate);
      } else {
        jumpAnimationFrameRef.current = undefined;
        finish();
      }
    };
    jumpAnimationFrameRef.current = requestAnimationFrame(animate);
  }, [cancelJumpAnimation]);

  return useMemo(() => ({
    capturePrependAnchor,
    jumpToLatest,
    messageListRef,
    moreBelow,
    onKeyDown,
    onPointerCancel: finishPointerGesture,
    onPointerDown,
    onPointerUp: finishPointerGesture,
    onScroll,
    onWheel,
    showJumpToLatest,
  }), [
    capturePrependAnchor,
    finishPointerGesture,
    jumpToLatest,
    moreBelow,
    onKeyDown,
    onPointerDown,
    onScroll,
    onWheel,
    showJumpToLatest,
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
  // Firefox overlay scrollbars occupy no layout width, so use the styled
  // scrollbar width as a right-edge hit target when geometry reports zero.
  const scrollbarWidth = Math.max(
    event.currentTarget.offsetWidth - event.currentTarget.clientWidth,
    OVERLAY_SCROLLBAR_HIT_WIDTH_PX,
  );
  const bounds = event.currentTarget.getBoundingClientRect();
  return event.clientX >= bounds.right - scrollbarWidth && event.clientX <= bounds.right;
}

function isAtLatest(messageList: HTMLDivElement) {
  return distanceFromLatest(messageList) <= 2;
}

function distanceFromLatest(messageList: HTMLDivElement) {
  return messageList.scrollHeight - messageList.scrollTop - messageList.clientHeight;
}

function latestScrollTop(messageList: HTMLDivElement) {
  return Math.max(0, messageList.scrollHeight - messageList.clientHeight);
}

function prefersReducedMotion() {
  return typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}
