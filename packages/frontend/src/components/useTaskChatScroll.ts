import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { KeyboardEvent, PointerEvent, UIEvent, WheelEvent } from "react";
import {
  chatFollowModeForPosition,
  initialTaskScrollTop,
  scrollTopAfterPrependedContent,
  scrollTopForGeneratedContent,
} from "./TaskViewModel";
import {
  chatScrollGeometry,
  TaskChatScrollDiagnostics,
  type TaskChatScrollDiagnosticContext,
} from "./taskChatScrollDiagnostics";

type ScrollOwnership = "following" | "reading";
const USER_SCROLL_INTENT_WINDOW_MS = 500;
const SHOW_JUMP_TO_LATEST_DISTANCE_PX = 96;
const HIDE_JUMP_TO_LATEST_DISTANCE_PX = 48;
const JUMP_TO_LATEST_DURATION_MS = 180;

// Owns the Chat viewport policy. Geometry can initialize ownership, but only explicit user intent
// changes it afterward, so streamed layout changes cannot steal control from the reader.
export function useTaskChatScroll({
  diagnosticContext,
  generating,
  historySyncState,
  itemCount,
  onScrollTop,
  pendingPrepend,
  savedScrollTop,
  taskId,
}: {
  diagnosticContext: TaskChatScrollDiagnosticContext;
  generating: boolean;
  historySyncState?: "idle" | "checking" | "syncing" | "updated" | "failed";
  itemCount: number;
  onScrollTop: (scrollTop: number) => void;
  pendingPrepend: boolean;
  savedScrollTop?: number;
  taskId: string;
}) {
  const messageListRef = useRef<HTMLDivElement | null>(null);
  const diagnosticsRef = useRef<{ taskId: string; recorder: TaskChatScrollDiagnostics } | undefined>(undefined);
  const jumpAnimationFrameRef = useRef<number | undefined>(undefined);
  const prependAnchorRef = useRef<{ scrollHeight: number; scrollTop: number } | undefined>(undefined);
  const historyAnchorRef = useRef<{ scrollHeight: number; scrollTop: number; following: boolean } | undefined>(undefined);
  const lastScrollHeightRef = useRef<number | undefined>(undefined);
  const lastScrollTopRef = useRef<number | undefined>(undefined);
  const skipGeneratedFollowOnceRef = useRef(false);
  const scrollOwnershipRef = useRef<ScrollOwnership>("following");
  const pointerScrollActiveRef = useRef(false);
  const pendingPermissionIdsRef = useRef(diagnosticContext.pendingPermissions);
  const permissionLayoutRecoveryRef = useRef(false);
  const permissionLayoutRecoveryFrameRef = useRef<number | undefined>(undefined);
  const towardLatestIntentUntilRef = useRef(0);
  const [showJumpToLatest, setShowJumpToLatest] = useState(false);
  const setScrollOwnership = useCallback((ownership: ScrollOwnership, reason = "unspecified") => {
    if (scrollOwnershipRef.current === ownership) return;
    scrollOwnershipRef.current = ownership;
    diagnosticsRef.current?.recorder.recordOwnership(ownership, reason);
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
  const clearPermissionLayoutRecovery = useCallback(() => {
    permissionLayoutRecoveryRef.current = false;
    if (permissionLayoutRecoveryFrameRef.current === undefined) return;
    cancelAnimationFrame(permissionLayoutRecoveryFrameRef.current);
    permissionLayoutRecoveryFrameRef.current = undefined;
  }, []);

  // Scroll persistence feeds this hook's props. Restore only when task identity changes so user scrolling
  // cannot re-enable follow mode.
  useLayoutEffect(() => {
    const messageList = messageListRef.current;
    if (!messageList) return;
    if (diagnosticsRef.current?.taskId !== taskId) {
      diagnosticsRef.current = { taskId, recorder: new TaskChatScrollDiagnostics(taskId) };
    }
    const diagnostics = diagnosticsRef.current.recorder;
    diagnostics.recordLifecycle("mounted");
    cancelJumpAnimation();
    clearPermissionLayoutRecovery();
    pointerScrollActiveRef.current = false;
    pendingPermissionIdsRef.current = diagnosticContext.pendingPermissions;
    towardLatestIntentUntilRef.current = 0;
    const scrollTop = initialTaskScrollTop(savedScrollTop, messageList.scrollHeight);
    messageList.scrollTop = scrollTop;
    setScrollOwnership(chatFollowModeForPosition({
      scrollTop,
      scrollHeight: messageList.scrollHeight,
      clientHeight: messageList.clientHeight,
    }) ? "following" : "reading", "taskRestored");
    setShowJumpToLatest(distanceFromLatest(messageList) > SHOW_JUMP_TO_LATEST_DISTANCE_PX);
    lastScrollTopRef.current = scrollTop;
    lastScrollHeightRef.current = messageList.scrollHeight;
    diagnostics.recordGeometry(chatScrollGeometry(messageList));
    return () => {
      clearPermissionLayoutRecovery();
      diagnostics.recordLifecycle("unmounted");
    };
  }, [cancelJumpAnimation, clearPermissionLayoutRecovery, setScrollOwnership, taskId]);

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.addEventListener !== "function") return undefined;
    const finishPointerScroll = () => {
      pointerScrollActiveRef.current = false;
    };
    // Pointer release can occur outside Chat after a scrollbar drag. Window-level
    // cleanup prevents that completed gesture from owning later layout scrolls.
    window.addEventListener("pointerup", finishPointerScroll);
    window.addEventListener("pointercancel", finishPointerScroll);
    return () => {
      window.removeEventListener("pointerup", finishPointerScroll);
      window.removeEventListener("pointercancel", finishPointerScroll);
    };
  }, []);

  useLayoutEffect(() => {
    diagnosticsRef.current?.recorder.recordRender(diagnosticContext);
  });

  const pendingPermissionsKey = JSON.stringify(diagnosticContext.pendingPermissions);
  useLayoutEffect(() => {
    const nextPermissionIds = diagnosticContext.pendingPermissions;
    const permissionResolved = pendingPermissionIdsRef.current.some(
      (requestId) => !nextPermissionIds.includes(requestId),
    );
    pendingPermissionIdsRef.current = nextPermissionIds;
    if (permissionResolved) {
      // The terminal card and resumed activity can settle in the same browser
      // frame. Recovery is a one-paint layout transaction, not a time window
      // that could later override reader intent.
      clearPermissionLayoutRecovery();
      permissionLayoutRecoveryRef.current = true;
      if (typeof requestAnimationFrame === "function") {
        permissionLayoutRecoveryFrameRef.current = requestAnimationFrame(() => {
          permissionLayoutRecoveryRef.current = false;
          permissionLayoutRecoveryFrameRef.current = undefined;
        });
      }
    }
  }, [clearPermissionLayoutRecovery, pendingPermissionsKey]);

  useLayoutEffect(() => {
    const messageList = messageListRef.current;
    if (!messageList) return;
    if (historySyncState === "checking" || historySyncState === "syncing") {
      historyAnchorRef.current = {
        scrollHeight: messageList.scrollHeight,
        scrollTop: messageList.scrollTop,
        following: scrollOwnershipRef.current === "following",
      };
      return;
    }
    const anchor = historyAnchorRef.current;
    if (!anchor) return;
    messageList.scrollTop = anchor.following
      ? messageList.scrollHeight
      : scrollTopAfterPrependedContent({
          previousScrollHeight: anchor.scrollHeight,
          previousScrollTop: anchor.scrollTop,
          nextScrollHeight: messageList.scrollHeight,
        });
    lastScrollTopRef.current = messageList.scrollTop;
    lastScrollHeightRef.current = messageList.scrollHeight;
    historyAnchorRef.current = undefined;
    skipGeneratedFollowOnceRef.current = true;
    updateJumpToLatestVisibility(messageList);
  }, [historySyncState, updateJumpToLatestVisibility]);

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
    if (pointerScrollActiveRef.current && previousScrollTop !== undefined) {
      if (scrollTop > previousScrollTop) {
        towardLatestIntentUntilRef.current = Date.now() + USER_SCROLL_INTENT_WINDOW_MS;
      } else if (scrollTop < previousScrollTop) {
        towardLatestIntentUntilRef.current = 0;
      }
    }
    if (
      previousScrollTop !== undefined
      && scrollTop < previousScrollTop
      && scrollOwnershipRef.current === "following"
      && !pointerScrollActiveRef.current
      && permissionLayoutRecoveryRef.current
      && lastScrollHeightRef.current !== undefined
      && messageList.scrollHeight !== lastScrollHeightRef.current
      && !isAtLatest(messageList)
    ) {
      // Permission resolution can contract the list and then append resumed work
      // before the browser delivers its scroll event. Keep layout movement from
      // masquerading as user intent and stranding a following viewport.
      messageList.scrollTop = messageList.scrollHeight;
      lastScrollTopRef.current = messageList.scrollTop;
      lastScrollHeightRef.current = messageList.scrollHeight;
      clearPermissionLayoutRecovery();
      diagnosticsRef.current?.recorder.recordGeometry(chatScrollGeometry(messageList));
      updateJumpToLatestVisibility(messageList);
      onScrollTop(messageList.scrollTop);
      return;
    }
    if (
      previousScrollTop !== undefined
      && scrollTop < previousScrollTop
      && (pointerScrollActiveRef.current || !isAtLatest(messageList))
    ) {
      setScrollOwnership("reading", "scrollTopDecreased");
    } else if (
      previousScrollTop !== undefined
      && scrollTop > previousScrollTop
      && scrollOwnershipRef.current === "reading"
      && isAtLatest(messageList)
      && (pointerScrollActiveRef.current || Date.now() <= towardLatestIntentUntilRef.current)
    ) {
      towardLatestIntentUntilRef.current = 0;
      setScrollOwnership("following", "reachedLatestWithIntent");
    }
    lastScrollTopRef.current = scrollTop;
    diagnosticsRef.current?.recorder.recordGeometry(chatScrollGeometry(messageList));
    if (historyAnchorRef.current) {
      historyAnchorRef.current = {
        scrollHeight: messageList.scrollHeight,
        scrollTop,
        following: scrollOwnershipRef.current === "following",
      };
    }
    updateJumpToLatestVisibility(messageList);
    onScrollTop(scrollTop);
  }, [clearPermissionLayoutRecovery, onScrollTop, setScrollOwnership, updateJumpToLatestVisibility]);

  const onWheel = useCallback((event: WheelEvent<HTMLDivElement>) => {
    cancelJumpAnimation();
    if (event.deltaY < 0) {
      diagnosticsRef.current?.recorder.recordIntent("towardEarlier");
      towardLatestIntentUntilRef.current = 0;
      setScrollOwnership("reading", "wheelTowardEarlier");
      return;
    }
    if (event.deltaY > 0) {
      diagnosticsRef.current?.recorder.recordIntent("towardLatest");
      towardLatestIntentUntilRef.current = Date.now() + USER_SCROLL_INTENT_WINDOW_MS;
    }
  }, [cancelJumpAnimation, setScrollOwnership]);

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
    diagnosticsRef.current?.recorder.recordIntent(
      direction === "towardEarlier" ? "towardEarlier" : "towardLatest",
    );
    if (direction === "towardEarlier") {
      towardLatestIntentUntilRef.current = 0;
      setScrollOwnership("reading", "keyboardTowardEarlier");
      return;
    }
    towardLatestIntentUntilRef.current = Date.now() + USER_SCROLL_INTENT_WINDOW_MS;
    const messageList = messageListRef.current;
    if (messageList && isAtLatest(messageList)) {
      towardLatestIntentUntilRef.current = 0;
      setScrollOwnership("following", "keyboardReachedLatest");
    }
  }, [cancelJumpAnimation, setScrollOwnership]);

  const onPointerDown = useCallback((_event: PointerEvent<HTMLDivElement>) => {
    cancelJumpAnimation();
    // Pointer ownership covers touch gestures and mouse scrollbar dragging.
    pointerScrollActiveRef.current = true;
  }, [cancelJumpAnimation]);

  const finishPointerScroll = useCallback((_event: PointerEvent<HTMLDivElement>) => {
    pointerScrollActiveRef.current = false;
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
    towardLatestIntentUntilRef.current = 0;
    setScrollOwnership("following", "jumpToLatest");
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
    onKeyDown,
    onPointerCancel: finishPointerScroll,
    onPointerDown,
    onPointerUp: finishPointerScroll,
    onScroll,
    onWheel,
    showJumpToLatest,
  };
}

function keyboardScrollDirection(key: string, shiftKey: boolean) {
  if (key === "PageUp" || key === "Home" || key === "ArrowUp" || (key === " " && shiftKey)) {
    return "towardEarlier" as const;
  }
  if (key === "PageDown" || key === "End" || key === "ArrowDown" || (key === " " && !shiftKey)) {
    return "towardLatest" as const;
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
