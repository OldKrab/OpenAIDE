import { useCallback, useLayoutEffect, useRef, useState } from "react";
import type { PointerEvent, UIEvent, WheelEvent } from "react";
import {
  chatFollowModeForPosition,
  initialTaskScrollTop,
  scrollTopAfterPrependedContent,
  scrollTopForGeneratedContent,
} from "./TaskViewModel";

type ScrollOwnership = "following" | "reading";
const USER_SCROLL_INTENT_WINDOW_MS = 500;
const SHOW_JUMP_TO_LATEST_DISTANCE_PX = 96;
const HIDE_JUMP_TO_LATEST_DISTANCE_PX = 48;
const JUMP_TO_LATEST_DURATION_MS = 180;

// Owns the Chat viewport policy. Geometry can initialize ownership, but only explicit user intent
// changes it afterward, so streamed layout changes cannot steal control from the reader.
export function useTaskChatScroll({
  generating,
  itemCount,
  onScrollTop,
  pendingPrepend,
  savedScrollTop,
  taskId,
}: {
  generating: boolean;
  itemCount: number;
  onScrollTop: (scrollTop: number) => void;
  pendingPrepend: boolean;
  savedScrollTop?: number;
  taskId: string;
}) {
  const messageListRef = useRef<HTMLDivElement | null>(null);
  const jumpAnimationFrameRef = useRef<number | undefined>(undefined);
  const prependAnchorRef = useRef<{ scrollHeight: number; scrollTop: number } | undefined>(undefined);
  const lastScrollHeightRef = useRef<number | undefined>(undefined);
  const lastScrollTopRef = useRef<number | undefined>(undefined);
  const skipGeneratedFollowOnceRef = useRef(false);
  const scrollOwnershipRef = useRef<ScrollOwnership>("following");
  const touchScrollActiveRef = useRef(false);
  const towardLatestIntentUntilRef = useRef(0);
  const [showJumpToLatest, setShowJumpToLatest] = useState(false);
  const setScrollOwnership = useCallback((ownership: ScrollOwnership) => {
    scrollOwnershipRef.current = ownership;
  }, []);
  const updateJumpToLatestVisibility = useCallback((messageList: HTMLDivElement) => {
    const distanceFromBottom = distanceFromLatest(messageList);
    setShowJumpToLatest((visible) => (
      visible
        ? distanceFromBottom > HIDE_JUMP_TO_LATEST_DISTANCE_PX
        : distanceFromBottom > SHOW_JUMP_TO_LATEST_DISTANCE_PX
    ));
  }, []);
  const cancelJumpAnimation = useCallback(() => {
    if (jumpAnimationFrameRef.current === undefined) return;
    cancelAnimationFrame(jumpAnimationFrameRef.current);
    jumpAnimationFrameRef.current = undefined;
  }, []);

  // Scroll persistence feeds this hook's props. Restore only when task identity changes so user scrolling
  // cannot re-enable follow mode.
  useLayoutEffect(() => {
    const messageList = messageListRef.current;
    if (!messageList) return;
    cancelJumpAnimation();
    const scrollTop = initialTaskScrollTop(savedScrollTop, messageList.scrollHeight);
    messageList.scrollTop = scrollTop;
    setScrollOwnership(chatFollowModeForPosition({
      scrollTop,
      scrollHeight: messageList.scrollHeight,
      clientHeight: messageList.clientHeight,
    }) ? "following" : "reading");
    setShowJumpToLatest(distanceFromLatest(messageList) > SHOW_JUMP_TO_LATEST_DISTANCE_PX);
    lastScrollTopRef.current = scrollTop;
    lastScrollHeightRef.current = messageList.scrollHeight;
  }, [cancelJumpAnimation, setScrollOwnership, taskId]);

  useLayoutEffect(() => {
    const messageList = messageListRef.current;
    const anchor = prependAnchorRef.current;
    if (!messageList || !anchor) return;
    if (pendingPrepend && messageList.scrollHeight === anchor.scrollHeight) return;
    messageList.scrollTop = scrollTopAfterPrependedContent({
      previousScrollHeight: anchor.scrollHeight,
      previousScrollTop: anchor.scrollTop,
      nextScrollHeight: messageList.scrollHeight,
    });
    lastScrollTopRef.current = messageList.scrollTop;
    lastScrollHeightRef.current = messageList.scrollHeight;
    updateJumpToLatestVisibility(messageList);
    prependAnchorRef.current = undefined;
    skipGeneratedFollowOnceRef.current = true;
  }, [itemCount, pendingPrepend, updateJumpToLatestVisibility]);

  useLayoutEffect(() => {
    const messageList = messageListRef.current;
    if (!messageList) return;
    if (skipGeneratedFollowOnceRef.current) {
      skipGeneratedFollowOnceRef.current = false;
      return;
    }
    const contentGrew = lastScrollHeightRef.current !== undefined
      && messageList.scrollHeight > lastScrollHeightRef.current;
    lastScrollHeightRef.current = messageList.scrollHeight;
    const scrollTop = scrollTopForGeneratedContent({
      followMode: scrollOwnershipRef.current === "following",
      // Final output can arrive in the same snapshot that marks the Task inactive.
      generating: generating || contentGrew,
      scrollHeight: messageList.scrollHeight,
    });
    if (scrollTop !== undefined) {
      messageList.scrollTop = scrollTop;
      lastScrollTopRef.current = messageList.scrollTop;
    }
    // Button visibility follows geometry, independently of whether streamed content may auto-follow.
    updateJumpToLatestVisibility(messageList);
  });

  const onScroll = useCallback((event: UIEvent<HTMLDivElement>) => {
    const messageList = event.currentTarget;
    const scrollTop = messageList.scrollTop;
    const previousScrollTop = lastScrollTopRef.current;
    if (touchScrollActiveRef.current && previousScrollTop !== undefined) {
      if (scrollTop > previousScrollTop) {
        towardLatestIntentUntilRef.current = Date.now() + USER_SCROLL_INTENT_WINDOW_MS;
      } else if (scrollTop < previousScrollTop) {
        towardLatestIntentUntilRef.current = 0;
      }
    }
    if (previousScrollTop !== undefined && scrollTop < previousScrollTop) {
      setScrollOwnership("reading");
    } else if (
      previousScrollTop !== undefined
      && scrollTop > previousScrollTop
      && scrollOwnershipRef.current === "reading"
      && isAtLatest(messageList)
      && (touchScrollActiveRef.current || Date.now() <= towardLatestIntentUntilRef.current)
    ) {
      towardLatestIntentUntilRef.current = 0;
      setScrollOwnership("following");
    }
    lastScrollTopRef.current = scrollTop;
    updateJumpToLatestVisibility(messageList);
    onScrollTop(scrollTop);
  }, [onScrollTop, setScrollOwnership, updateJumpToLatestVisibility]);

  const onWheel = useCallback((event: WheelEvent<HTMLDivElement>) => {
    cancelJumpAnimation();
    if (event.deltaY < 0) {
      towardLatestIntentUntilRef.current = 0;
      setScrollOwnership("reading");
      return;
    }
    if (event.deltaY > 0) {
      towardLatestIntentUntilRef.current = Date.now() + USER_SCROLL_INTENT_WINDOW_MS;
    }
  }, [cancelJumpAnimation, setScrollOwnership]);

  const onPointerDown = useCallback((event: PointerEvent<HTMLDivElement>) => {
    cancelJumpAnimation();
    if (event.pointerType === "touch") touchScrollActiveRef.current = true;
  }, [cancelJumpAnimation]);

  const finishPointerScroll = useCallback((event: PointerEvent<HTMLDivElement>) => {
    if (event.pointerType !== "touch") return;
    touchScrollActiveRef.current = false;
  }, []);

  const capturePrependAnchor = useCallback(() => {
    const messageList = messageListRef.current;
    if (!messageList) return;
    prependAnchorRef.current = {
      scrollHeight: messageList.scrollHeight,
      scrollTop: messageList.scrollTop,
    };
  }, []);

  const jumpToLatest = useCallback(() => {
    const messageList = messageListRef.current;
    if (!messageList) return;
    cancelJumpAnimation();
    setScrollOwnership("following");
    setShowJumpToLatest(false);
    const startScrollTop = messageList.scrollTop;
    if (prefersReducedMotion()) {
      messageList.scrollTop = messageList.scrollHeight;
      lastScrollTopRef.current = messageList.scrollTop;
      onScrollTop(messageList.scrollTop);
      return;
    }
    const startedAt = performance.now();
    const animate = (now: number) => {
      const progress = Math.min(1, (now - startedAt) / JUMP_TO_LATEST_DURATION_MS);
      const easedProgress = 1 - ((1 - progress) ** 4);
      const targetScrollTop = messageList.scrollHeight - messageList.clientHeight;
      messageList.scrollTop = startScrollTop + ((targetScrollTop - startScrollTop) * easedProgress);
      lastScrollTopRef.current = messageList.scrollTop;
      if (progress < 1) {
        jumpAnimationFrameRef.current = requestAnimationFrame(animate);
        return;
      }
      jumpAnimationFrameRef.current = undefined;
      onScrollTop(messageList.scrollTop);
    };
    jumpAnimationFrameRef.current = requestAnimationFrame(animate);
  }, [cancelJumpAnimation, onScrollTop, setScrollOwnership]);

  return {
    capturePrependAnchor,
    jumpToLatest,
    messageListRef,
    onPointerCancel: finishPointerScroll,
    onPointerDown,
    onPointerUp: finishPointerScroll,
    onScroll,
    onWheel,
    showJumpToLatest,
  };
}

function isAtLatest(messageList: HTMLDivElement) {
  return distanceFromLatest(messageList) <= 2;
}

function distanceFromLatest(messageList: HTMLDivElement) {
  return messageList.scrollHeight - messageList.scrollTop - messageList.clientHeight;
}

function prefersReducedMotion() {
  return typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}
