import { useEffect, useRef, useState, type RefObject, type TouchEvent as ReactTouchEvent } from "react";

/** Finger travel past the top of the list that triggers a refresh on release. */
export const PULL_REFRESH_THRESHOLD_PX = 64;
/** Pulls resist after the threshold so the indicator stops near it. */
const PULL_RESISTANCE = 0.5;
/** Longest a pull-started refresh shows its indicator if the caller never reports it busy. */
const REFRESH_INDICATOR_TIMEOUT_MS = 10_000;

/**
 * Pull-to-refresh for a touch-scrolled list. A refresh starts only when a downward
 * pull begins with the list scrolled to its top and is released past the threshold;
 * a mostly horizontal drag is left to the drawer's swipe. The caller owns the refresh
 * and its `busy` state; this hook owns only the transient pull distance and whether a
 * refresh it started is still running, so background refreshes show no indicator.
 */
export function usePullToRefresh(
  listRef: RefObject<HTMLElement | null>,
  onRefresh: () => void,
  { busy, enabled }: { busy: boolean; enabled: boolean },
) {
  const [pull, setPull] = useState(0);
  const [refreshing, setRefreshing] = useState(false);
  const sawBusyRef = useRef(false);
  const startRef = useRef<{ x: number; y: number; vertical?: boolean } | undefined>(undefined);

  useEffect(() => {
    if (!refreshing) return undefined;
    if (busy) sawBusyRef.current = true;
    else if (sawBusyRef.current) setRefreshing(false);
    const timeout = setTimeout(() => setRefreshing(false), REFRESH_INDICATOR_TIMEOUT_MS);
    return () => clearTimeout(timeout);
  }, [busy, refreshing]);

  const reset = () => {
    startRef.current = undefined;
    setPull(0);
  };

  return {
    /** Distance the indicator is pulled down, in CSS pixels. */
    pull,
    armed: pull >= PULL_REFRESH_THRESHOLD_PX,
    /** A refresh this pull started has not finished yet. */
    refreshing,
    handlers: {
      onTouchStart(event: ReactTouchEvent<HTMLElement>) {
        const touch = event.touches[0];
        startRef.current = enabled && !busy && event.touches.length === 1 && touch && (listRef.current?.scrollTop ?? 1) <= 0
          ? { x: touch.clientX, y: touch.clientY }
          : undefined;
      },
      onTouchMove(event: ReactTouchEvent<HTMLElement>) {
        const start = startRef.current;
        const touch = event.touches[0];
        if (!start || !touch) return;
        const dx = touch.clientX - start.x;
        const dy = touch.clientY - start.y;
        if (start.vertical === undefined) {
          if (Math.abs(dx) < 6 && Math.abs(dy) < 6) return;
          start.vertical = dy > 0 && dy > Math.abs(dx) * 1.2;
        }
        if (!start.vertical) {
          reset();
          return;
        }
        const distance = Math.max(0, dy);
        setPull(distance <= PULL_REFRESH_THRESHOLD_PX
          ? distance
          : PULL_REFRESH_THRESHOLD_PX + (distance - PULL_REFRESH_THRESHOLD_PX) * PULL_RESISTANCE);
      },
      onTouchEnd() {
        if (startRef.current?.vertical && pull >= PULL_REFRESH_THRESHOLD_PX) {
          sawBusyRef.current = false;
          setRefreshing(true);
          onRefresh();
        }
        reset();
      },
      onTouchCancel: reset,
    },
  };
}
