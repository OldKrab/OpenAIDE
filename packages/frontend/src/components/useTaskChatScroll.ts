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
  const prependAnchorRef = useRef<{ scrollHeight: number; scrollTop: number } | undefined>(undefined);
  const lastScrollTopRef = useRef<number | undefined>(undefined);
  const skipGeneratedFollowOnceRef = useRef(false);
  const scrollOwnershipRef = useRef<ScrollOwnership>("following");
  const touchScrollActiveRef = useRef(false);
  const towardLatestIntentUntilRef = useRef(0);
  const [atLatest, setAtLatest] = useState(true);
  const setScrollOwnership = useCallback((ownership: ScrollOwnership) => {
    scrollOwnershipRef.current = ownership;
  }, []);

  // Scroll persistence feeds this hook's props. Restore only when task identity changes so user scrolling
  // cannot re-enable follow mode.
  useLayoutEffect(() => {
    const messageList = messageListRef.current;
    if (!messageList) return;
    const scrollTop = initialTaskScrollTop(savedScrollTop, messageList.scrollHeight);
    messageList.scrollTop = scrollTop;
    setScrollOwnership(chatFollowModeForPosition({
      scrollTop,
      scrollHeight: messageList.scrollHeight,
      clientHeight: messageList.clientHeight,
    }) ? "following" : "reading");
    setAtLatest(isAtLatest(messageList));
    lastScrollTopRef.current = scrollTop;
  }, [setScrollOwnership, taskId]);

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
    setAtLatest(isAtLatest(messageList));
    prependAnchorRef.current = undefined;
    skipGeneratedFollowOnceRef.current = true;
  }, [itemCount, pendingPrepend]);

  useLayoutEffect(() => {
    const messageList = messageListRef.current;
    if (!messageList) return;
    if (skipGeneratedFollowOnceRef.current) {
      skipGeneratedFollowOnceRef.current = false;
      return;
    }
    const scrollTop = scrollTopForGeneratedContent({
      followMode: scrollOwnershipRef.current === "following",
      generating,
      scrollHeight: messageList.scrollHeight,
    });
    if (scrollTop !== undefined) {
      messageList.scrollTop = scrollTop;
      lastScrollTopRef.current = messageList.scrollTop;
    }
    // Button visibility follows geometry, independently of whether streamed content may auto-follow.
    setAtLatest(isAtLatest(messageList));
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
    setAtLatest(isAtLatest(messageList));
    onScrollTop(scrollTop);
  }, [onScrollTop, setScrollOwnership]);

  const onWheel = useCallback((event: WheelEvent<HTMLDivElement>) => {
    if (event.deltaY < 0) {
      towardLatestIntentUntilRef.current = 0;
      setScrollOwnership("reading");
      return;
    }
    if (event.deltaY > 0) {
      towardLatestIntentUntilRef.current = Date.now() + USER_SCROLL_INTENT_WINDOW_MS;
    }
  }, [setScrollOwnership]);

  const onPointerDown = useCallback((event: PointerEvent<HTMLDivElement>) => {
    if (event.pointerType === "touch") touchScrollActiveRef.current = true;
  }, []);

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
    setScrollOwnership("following");
    messageList.scrollTop = messageList.scrollHeight;
    lastScrollTopRef.current = messageList.scrollTop;
    setAtLatest(true);
    onScrollTop(messageList.scrollTop);
  }, [onScrollTop, setScrollOwnership]);

  return {
    capturePrependAnchor,
    jumpToLatest,
    messageListRef,
    onPointerCancel: finishPointerScroll,
    onPointerDown,
    onPointerUp: finishPointerScroll,
    onScroll,
    onWheel,
    showJumpToLatest: !atLatest,
  };
}

function isAtLatest(messageList: HTMLDivElement) {
  return messageList.scrollHeight - messageList.scrollTop - messageList.clientHeight <= 2;
}
