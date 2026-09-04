import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent, PointerEvent, UIEvent, WheelEvent } from "react";
import { elementScroll, useVirtualizer } from "@tanstack/react-virtual";
import type { VirtualizerOptions } from "@tanstack/react-virtual";
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
type PendingPreviousUserMessage = {
  itemKeys: readonly string[];
  retainedFirstKey: string;
  sawPending: boolean;
};

export type UserMessageAnchor = {
  key: string;
  rowIndex: number;
  text: string;
};

export type UserMessageNavigation = {
  anchors: readonly UserMessageAnchor[];
  announcement?: string;
  currentIndex: number;
  goFirst: () => void;
  goLast: () => void;
  goNext: () => void;
  goPrevious: () => void;
  hasEarlier: boolean;
  navigateTo: (anchor: UserMessageAnchor) => void;
  pendingPrevious: boolean;
  targetKey?: string;
};

const SHOW_JUMP_TO_LATEST_DISTANCE_PX = 96;
const HIDE_JUMP_TO_LATEST_DISTANCE_PX = 48;
const OVERLAY_SCROLLBAR_HIT_WIDTH_PX = 10;
const AUTO_FILL_HISTORY_BUFFER_PX = 120;
const MAX_AUTO_FILL_PAGES = 4;
const CHAT_ROW_ESTIMATE_PX = 72;
const CHAT_ROW_GAP_PX = 8;
const CHAT_START_PADDING_PX = 24;
const CHAT_END_PADDING_PX = 64;
const CHAT_INITIAL_RECT = { width: 760, height: 600 };
const USER_MESSAGE_SELECTION_HYSTERESIS_PX = 24;
const USER_MESSAGE_ORIENTATION_DURATION_MS = 1_800;
const USER_MESSAGE_SCROLL_FRAME_COUNT = 24;

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
  userMessageAnchors?: readonly UserMessageAnchor[];
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
    userMessageAnchors = [],
  } = options;
  const autoFillPageCountRef = useRef(0);
  const autoFillCursorRef = useRef<string | undefined>(undefined);
  const lastMessageKeyRef = useRef<string | undefined>(latestMessageKey);
  const lastScrollTopRef = useRef<number | undefined>(undefined);
  const messageListRef = useRef<HTMLDivElement | null>(null);
  const onScrollStateRef = useRef(onScrollState);
  const pointerGestureRef = useRef<PointerScrollGesture | undefined>(undefined);
  const pendingPreviousUserMessageRef = useRef<PendingPreviousUserMessage | undefined>(undefined);
  const prependAnchorRef = useRef<PrependAnchor | undefined>(undefined);
  const prependCorrectionFrameRef = useRef<number | undefined>(undefined);
  // Keep rapid repeated navigation relative to the chosen anchor until user
  // input takes ownership of Chat scrolling again.
  const userMessageNavigationLockRef = useRef<string | undefined>(undefined);
  const userMessageScrollActiveRef = useRef(false);
  const userMessageOrientationTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const scrollIntentRef = useRef<ScrollIntent | undefined>(undefined);
  const smoothScrollRef = useRef<{
    frameCount: number;
    frameId: number;
    instance: Parameters<NonNullable<VirtualizerOptions<HTMLDivElement, HTMLDivElement>["scrollToFn"]>>[2];
    startOffset: number;
    targetOffset: number;
  } | undefined>(undefined);
  const scrollOwnershipRef = useRef<TaskChatScrollState["ownership"]>(
    savedScrollState?.ownership ?? "following",
  );
  const [scrollOwnership, setScrollOwnershipState] = useState<TaskChatScrollState["ownership"]>(
    savedScrollState?.ownership ?? "following",
  );
  const [showJumpToLatest, setShowJumpToLatest] = useState(false);
  const [moreBelow, setMoreBelow] = useState(false);
  const [currentUserMessageKey, setCurrentUserMessageKey] = useState<string | undefined>(undefined);
  const [userMessageAnnouncement, setUserMessageAnnouncement] = useState<string | undefined>(undefined);
  const [userMessageScrollTargetIndex, setUserMessageScrollTargetIndex] = useState<number | undefined>(undefined);
  const [userMessageTargetKey, setUserMessageTargetKey] = useState<string | undefined>(undefined);
  onScrollStateRef.current = onScrollState;

  const getItemKey = useCallback(
    (index: number) => itemKeys[index] ?? `missing-chat-row:${index}`,
    [itemKeys],
  );
  const scrollToFn = useCallback<NonNullable<
    VirtualizerOptions<HTMLDivElement, HTMLDivElement>["scrollToFn"]
  >>((offset, scrollOptions, instance) => {
    const active = smoothScrollRef.current;
    if (active && userMessageScrollActiveRef.current) {
      // TanStack may downgrade a measurement correction to `auto` near the
      // destination. It is still the same semantic navigation, so retarget
      // the fixed-duration motion instead of truncating it.
      active.targetOffset = offset;
      return;
    }
    if (userMessageScrollActiveRef.current && scrollOptions.behavior !== "smooth") {
      // Raising overscan measures more rows before the requested navigation
      // starts. Ignore those anchoring corrections: applying them here would
      // move to a newly measured offset before the first animation frame.
      return;
    }
    if (scrollOptions.behavior !== "smooth" || typeof requestAnimationFrame !== "function") {
      if (active && typeof cancelAnimationFrame === "function") cancelAnimationFrame(active.frameId);
      smoothScrollRef.current = undefined;
      elementScroll(offset, scrollOptions, instance);
      return;
    }
    if (!userMessageScrollActiveRef.current) {
      elementScroll(offset, scrollOptions, instance);
      return;
    }

    const state = {
      frameCount: 0,
      frameId: 0,
      instance,
      startOffset: messageListRef.current?.scrollTop ?? 0,
      targetOffset: offset,
    };
    const animate = () => {
      if (smoothScrollRef.current !== state) return;
      state.frameCount += 1;
      // Advance by painted frames instead of elapsed time. A delayed Firefox
      // frame must not skip ahead of the virtual rows React has prepared.
      const progress = Math.min(1, state.frameCount / USER_MESSAGE_SCROLL_FRAME_COUNT);
      elementScroll(
        state.startOffset + (state.targetOffset - state.startOffset) * progress,
        { behavior: "auto" },
        state.instance,
      );
      if (progress < 1) {
        state.frameId = requestAnimationFrame(animate);
      } else {
        smoothScrollRef.current = undefined;
        userMessageScrollActiveRef.current = false;
        setUserMessageScrollTargetIndex(undefined);
        const messageList = messageListRef.current;
        if (messageList) {
          onScrollStateRef.current({ ownership: "reading", scrollTop: messageList.scrollTop });
        }
      }
    };
    smoothScrollRef.current = state;
    state.frameId = requestAnimationFrame(animate);
  }, []);
  const virtualizer = useVirtualizer<HTMLDivElement, HTMLDivElement>({
    // Initial Chat must begin at its first row. Following later messages is
    // handled explicitly below, after the reader's intent is known.
    anchorTo: "start",
    count: itemKeys.length,
    directDomUpdates: true,
    estimateSize: () => CHAT_ROW_ESTIMATE_PX,
    followOnAppend: scrollOwnership === "following",
    gap: CHAT_ROW_GAP_PX,
    getItemKey,
    getScrollElement: () => messageListRef.current,
    // Avoid a blank first render before the App Shell viewport is measured.
    initialRect: CHAT_INITIAL_RECT,
    overscan: userMessageScrollTargetIndex === undefined ? 4 : 12,
    paddingStart: CHAT_START_PADDING_PX,
    paddingEnd: CHAT_END_PADDING_PX,
    scrollEndThreshold: 2,
    scrollToFn,
    // Range changes must commit before paint so a far navigation never exposes
    // the empty space between the old and next virtual ranges.
    useFlushSync: true,
  });

  const updateCurrentUserMessage = useCallback(() => {
    const messageList = messageListRef.current;
    if (!messageList || userMessageAnchors.length === 0) {
      setCurrentUserMessageKey(undefined);
      return;
    }
    const lockedKey = userMessageNavigationLockRef.current;
    if (lockedKey && userMessageAnchors.some((anchor) => anchor.key === lockedKey)) {
      setCurrentUserMessageKey((existing) => existing === lockedKey ? existing : lockedKey);
      return;
    }
    const viewportTop = messageList.scrollTop;
    const viewportBottom = viewportTop + messageList.clientHeight;
    const viewportCenter = (viewportTop + viewportBottom) / 2;
    const virtualItemsByIndex = new Map(
      virtualizer.getVirtualItems().map((item) => [item.index, item]),
    );
    const visibleAnchors = userMessageAnchors.flatMap((anchor) => {
      const item = virtualItemsByIndex.get(anchor.rowIndex);
      if (!item || item.end <= viewportTop || item.start >= viewportBottom) return [];
      const distanceFromCenter = viewportCenter < item.start
        ? item.start - viewportCenter
        : viewportCenter > item.end
          ? viewportCenter - item.end
          : 0;
      return [{ anchor, distanceFromCenter }];
    });
    if (visibleAnchors.length > 0) {
      const closest = visibleAnchors.reduce((currentClosest, candidate) => (
        candidate.distanceFromCenter < currentClosest.distanceFromCenter ? candidate : currentClosest
      ));
      setCurrentUserMessageKey((existing) => {
        const existingCandidate = visibleAnchors.find(({ anchor }) => anchor.key === existing);
        const selected = existingCandidate
          && existingCandidate.distanceFromCenter
            <= closest.distanceFromCenter + USER_MESSAGE_SELECTION_HYSTERESIS_PX
          ? existingCandidate.anchor
          : closest.anchor;
        return existing === selected.key ? existing : selected.key;
      });
      return;
    }

    // An Agent response can fill the viewport without its initiating User
    // message remaining visible. In that gap, retain the preceding prompt as
    // the section owner until another User message enters the viewport.
    let current = userMessageAnchors[0];
    for (const anchor of userMessageAnchors) {
      const measuredOffset = virtualizer.getOffsetForIndex(anchor.rowIndex, "start")?.[0];
      const fallbackOffset = itemKeys.length > 1
        ? (anchor.rowIndex / (itemKeys.length - 1)) * virtualizer.getTotalSize()
        : 0;
      if ((measuredOffset ?? fallbackOffset) >= viewportTop) break;
      current = anchor;
    }
    setCurrentUserMessageKey((existing) => existing === current.key ? existing : current.key);
  }, [itemKeys.length, userMessageAnchors, virtualizer]);

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

  const cancelUserMessageScroll = useCallback(() => {
    if (!userMessageScrollActiveRef.current) return;
    userMessageScrollActiveRef.current = false;
    setUserMessageScrollTargetIndex(undefined);
    const messageList = messageListRef.current;
    if (messageList) {
      // Replace TanStack's tracked index target as well as stopping the
      // browser animation, otherwise measurement reconciliation can resume a
      // navigation that the reader interrupted.
      virtualizer.scrollToOffset(messageList.scrollTop, { behavior: "auto" });
    }
  }, [virtualizer]);

  useEffect(() => () => {
    if (prependCorrectionFrameRef.current !== undefined && typeof cancelAnimationFrame === "function") {
      cancelAnimationFrame(prependCorrectionFrameRef.current);
    }
    if (userMessageOrientationTimerRef.current !== undefined) {
      clearTimeout(userMessageOrientationTimerRef.current);
    }
    const smoothScroll = smoothScrollRef.current;
    if (smoothScroll && typeof cancelAnimationFrame === "function") {
      cancelAnimationFrame(smoothScroll.frameId);
      smoothScrollRef.current = undefined;
    }
    cancelUserMessageScroll();
  }, [cancelUserMessageScroll, taskId]);

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
    pendingPreviousUserMessageRef.current = undefined;
    userMessageNavigationLockRef.current = undefined;
    setCurrentUserMessageKey(undefined);
    setUserMessageAnnouncement(undefined);
    setUserMessageScrollTargetIndex(undefined);
    setUserMessageTargetKey(undefined);
    if (userMessageOrientationTimerRef.current !== undefined) {
      clearTimeout(userMessageOrientationTimerRef.current);
      userMessageOrientationTimerRef.current = undefined;
    }

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
      updateCurrentUserMessage();
      autoLoadEarlierIfUnderfilled();
    });
    observer.observe(messageList);
    return () => observer.disconnect();
  }, [autoLoadEarlierIfUnderfilled, taskId, updateCurrentUserMessage, updateJumpToLatestVisibility, virtualizer]);

  // ResizeObserver is present in every supported App Shell. Keep a render
  // fallback for tests and constrained embedded environments.
  useLayoutEffect(() => {
    if (typeof ResizeObserver === "function") return;
    if (scrollOwnershipRef.current === "following") virtualizer.scrollToEnd();
    updateJumpToLatestVisibility();
    updateCurrentUserMessage();
    autoLoadEarlierIfUnderfilled();
  });

  useLayoutEffect(() => {
    autoLoadEarlierIfUnderfilled();
  }, [autoLoadEarlierIfUnderfilled, itemKeys.length]);

  useLayoutEffect(() => {
    updateCurrentUserMessage();
  }, [itemKeys, taskId, updateCurrentUserMessage]);

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
    userMessageNavigationLockRef.current = undefined;
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
    if (userMessageScrollActiveRef.current) {
      lastScrollTopRef.current = messageList.scrollTop;
      return;
    }
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
    updateCurrentUserMessage();
    persistScrollState();
  }, [persistScrollState, setScrollIntent, setScrollOwnership, updateCurrentUserMessage, updateJumpToLatestVisibility, virtualizer]);

  const onWheel = useCallback((event: WheelEvent<HTMLDivElement>) => {
    if (event.deltaY === 0) return;
    cancelUserMessageScroll();
    userMessageNavigationLockRef.current = undefined;
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
  }, [cancelUserMessageScroll, setScrollIntent, setScrollOwnership, virtualizer]);

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
    cancelUserMessageScroll();
    userMessageNavigationLockRef.current = undefined;
    setScrollIntent(direction);
    if (direction === "towardEarlier") {
      setScrollOwnership("reading");
    } else if (virtualizer.isAtEnd(2)) {
      scrollIntentRef.current = undefined;
      setScrollOwnership("following");
    }
  }, [cancelUserMessageScroll, setScrollIntent, setScrollOwnership, virtualizer]);

  const onPointerDown = useCallback((event: PointerEvent<HTMLDivElement>) => {
    const gesture = event.pointerType === "touch"
      ? { kind: "touch" as const, lastClientY: event.clientY }
      : (isVerticalScrollbarPointer(event) ? { kind: "scrollbar" as const } : undefined);
    pointerGestureRef.current = gesture;
    if (gesture) {
      cancelUserMessageScroll();
      userMessageNavigationLockRef.current = undefined;
    }
    if (gesture?.kind === "scrollbar") setScrollOwnership("reading");
  }, [cancelUserMessageScroll, setScrollOwnership]);

  const jumpToLatest = useCallback(() => {
    scrollIntentRef.current = undefined;
    userMessageNavigationLockRef.current = undefined;
    setShowJumpToLatest(false);
    setScrollOwnership("following");
    virtualizer.scrollToEnd({ behavior: prefersReducedMotion() ? "auto" : "smooth" });
  }, [setScrollOwnership, virtualizer]);

  // Overlay expansion is reading intent: preserve the viewport before its scroll range changes.
  const pauseFollowing = useCallback(() => {
    setScrollOwnership("reading");
  }, [setScrollOwnership]);

  const currentUserMessageIndex = useMemo(() => {
    const index = userMessageAnchors.findIndex((anchor) => anchor.key === currentUserMessageKey);
    if (index >= 0) return index;
    return scrollOwnership === "following" ? Math.max(0, userMessageAnchors.length - 1) : 0;
  }, [currentUserMessageKey, scrollOwnership, userMessageAnchors]);

  const navigateToUserMessage = useCallback((anchor: UserMessageAnchor) => {
    const index = userMessageAnchors.findIndex((candidate) => candidate.key === anchor.key);
    if (index < 0) return;
    setScrollOwnership("reading");
    userMessageNavigationLockRef.current = anchor.key;
    setCurrentUserMessageKey(anchor.key);
    setUserMessageTargetKey(anchor.key);
    setUserMessageAnnouncement(`User message ${index + 1} of ${userMessageAnchors.length}.`);
    cancelUserMessageScroll();
    const behavior = prefersReducedMotion() ? "auto" : "smooth";
    userMessageScrollActiveRef.current = behavior === "smooth";
    // Keep the logical row as TanStack's target. The virtualizer reconciles
    // its offset as variable-height Chat rows are measured during travel.
    if (behavior === "smooth") {
      setUserMessageScrollTargetIndex(anchor.rowIndex);
    } else {
      virtualizer.scrollToIndex(anchor.rowIndex, { align: "start", behavior });
      persistScrollState("reading");
    }
    if (userMessageOrientationTimerRef.current !== undefined) {
      clearTimeout(userMessageOrientationTimerRef.current);
    }
    userMessageOrientationTimerRef.current = setTimeout(() => {
      userMessageScrollActiveRef.current = false;
      setUserMessageScrollTargetIndex(undefined);
      persistScrollState("reading");
      if (userMessageNavigationLockRef.current === anchor.key) {
        userMessageNavigationLockRef.current = undefined;
      }
      setUserMessageTargetKey((current) => current === anchor.key ? undefined : current);
      userMessageOrientationTimerRef.current = undefined;
    }, USER_MESSAGE_ORIENTATION_DURATION_MS);
  }, [cancelUserMessageScroll, persistScrollState, setScrollOwnership, userMessageAnchors, virtualizer]);

  // Commit the larger travel buffer before motion starts. It remains bounded,
  // unlike mounting the full route through a long or image-heavy Chat.
  useLayoutEffect(() => {
    if (userMessageScrollTargetIndex === undefined) return;
    virtualizer.scrollToIndex(userMessageScrollTargetIndex, {
      align: "start",
      behavior: "smooth",
    });
  }, [userMessageScrollTargetIndex, virtualizer]);


  const goPreviousUserMessage = useCallback(() => {
    if (userMessageAnchors.length === 0 || pendingPrepend) return;
    if (currentUserMessageIndex > 0) {
      navigateToUserMessage(userMessageAnchors[currentUserMessageIndex - 1]!);
      return;
    }
    if (!hasEarlier || !beforeCursor) return;
    const requestId = loadEarlier(beforeCursor);
    if (requestId === undefined) return;
    pendingPreviousUserMessageRef.current = {
      itemKeys: [...itemKeys],
      retainedFirstKey: userMessageAnchors[0]!.key,
      sawPending: false,
    };
  }, [beforeCursor, currentUserMessageIndex, hasEarlier, itemKeys, loadEarlier, navigateToUserMessage, pendingPrepend, userMessageAnchors]);

  const goNextUserMessage = useCallback(() => {
    const next = userMessageAnchors[currentUserMessageIndex + 1];
    if (next) navigateToUserMessage(next);
  }, [currentUserMessageIndex, navigateToUserMessage, userMessageAnchors]);

  const goFirstUserMessage = useCallback(() => {
    const first = userMessageAnchors[0];
    if (first) navigateToUserMessage(first);
  }, [navigateToUserMessage, userMessageAnchors]);

  const goLastUserMessage = useCallback(() => {
    const last = userMessageAnchors.at(-1);
    if (last) navigateToUserMessage(last);
  }, [navigateToUserMessage, userMessageAnchors]);

  useLayoutEffect(() => {
    const pending = pendingPreviousUserMessageRef.current;
    if (!pending) return;
    if (pendingPrepend) {
      pending.sawPending = true;
      return;
    }
    if (!pending.sawPending && sameKeys(pending.itemKeys, itemKeys)) return;
    pendingPreviousUserMessageRef.current = undefined;
    const retainedIndex = userMessageAnchors.findIndex(
      (anchor) => anchor.key === pending.retainedFirstKey,
    );
    if (retainedIndex > 0) navigateToUserMessage(userMessageAnchors[retainedIndex - 1]!);
  }, [itemKeys, navigateToUserMessage, pendingPrepend, userMessageAnchors]);

  const userMessageNavigation = useMemo<UserMessageNavigation>(() => ({
    anchors: userMessageAnchors,
    announcement: userMessageAnnouncement,
    currentIndex: currentUserMessageIndex,
    goFirst: goFirstUserMessage,
    goLast: goLastUserMessage,
    goNext: goNextUserMessage,
    goPrevious: goPreviousUserMessage,
    hasEarlier,
    navigateTo: navigateToUserMessage,
    pendingPrevious: pendingPrepend && pendingPreviousUserMessageRef.current !== undefined,
    targetKey: userMessageTargetKey,
  }), [
    currentUserMessageIndex,
    goFirstUserMessage,
    goLastUserMessage,
    goNextUserMessage,
    goPreviousUserMessage,
    hasEarlier,
    navigateToUserMessage,
    pendingPrepend,
    userMessageAnchors,
    userMessageAnnouncement,
    userMessageTargetKey,
  ]);

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
    userMessageNavigation,
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
    userMessageNavigation,
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
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}
