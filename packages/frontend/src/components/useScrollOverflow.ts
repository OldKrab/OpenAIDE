import { useCallback, useLayoutEffect, useState, type RefObject, type UIEvent } from "react";

const EDGE_THRESHOLD_PX = 2;

/** Tracks whether a scroll viewport has content beyond either visible edge. */
export function useScrollOverflow<T extends HTMLElement>(
  viewportRef: RefObject<T | null>,
  refreshKey?: unknown,
) {
  const [moreAbove, setMoreAbove] = useState(false);
  const [moreBelow, setMoreBelow] = useState(false);
  const update = useCallback((viewport = viewportRef.current) => {
    if (!viewport) return;
    const distance = viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight;
    setMoreAbove(viewport.scrollTop > EDGE_THRESHOLD_PX);
    setMoreBelow(distance > EDGE_THRESHOLD_PX);
  }, [viewportRef]);
  const onScroll = useCallback((event: UIEvent<T>) => {
    update(event.currentTarget);
  }, [update]);

  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    update(viewport);
    if (typeof ResizeObserver !== "function") return;

    const observer = new ResizeObserver(() => update(viewport));
    const observedChildren = new Set<Element>();
    const observeChildren = () => {
      const children = new Set(Array.from(viewport.children));
      for (const child of observedChildren) {
        if (children.has(child)) continue;
        observer.unobserve(child);
        observedChildren.delete(child);
      }
      for (const child of children) {
        if (observedChildren.has(child)) continue;
        observer.observe(child);
        observedChildren.add(child);
      }
      update(viewport);
    };
    observer.observe(viewport);
    observeChildren();
    const mutationObserver = typeof MutationObserver === "function"
      ? new MutationObserver(observeChildren)
      : undefined;
    mutationObserver?.observe(viewport, { childList: true, subtree: true });
    return () => {
      mutationObserver?.disconnect();
      observer.disconnect();
    };
  }, [refreshKey, update, viewportRef]);

  return { moreAbove, moreBelow, onScroll, update };
}
