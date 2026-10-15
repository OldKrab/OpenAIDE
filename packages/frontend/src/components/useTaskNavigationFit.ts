import { useLayoutEffect, useRef, useState, type RefObject } from "react";

/** One Project group whose history rows may shrink to fit the Task Navigation viewport. */
export type TaskNavigationFitGroup = {
  key: string;
  /** History rows the group could show; live rows are fixed cost. */
  history: number;
  /** Upper bound from the per-Project presentation limit. */
  cap: number;
  /** Load More stays visible regardless of history, e.g. while App Server may hold more rows. */
  moreAlways: boolean;
};

/**
 * Shares the space left after headers and live rows between Project groups one history row at a
 * time, so a group with long history cannot push another group's live rows out of view. A group
 * that keeps hidden history pays for its Load More button.
 */
export function allocateHistoryRows(
  groups: TaskNavigationFitGroup[],
  availablePx: number,
  rowPx: number,
  morePx: number,
): Map<string, number> {
  const shown = new Map(groups.map((group) => [group.key, 0]));
  let available = availablePx - groups.filter((group) => group.history > 0 || group.moreAlways).length * morePx;
  let progressed = true;
  while (progressed) {
    progressed = false;
    for (const group of groups) {
      const count = shown.get(group.key)!;
      if (count >= Math.min(group.history, group.cap)) continue;
      const revealsAll = count + 1 === group.history && !group.moreAlways;
      const cost = rowPx - (revealsAll ? morePx : 0);
      if (cost > available) continue;
      shown.set(group.key, count + 1);
      available -= cost;
      progressed = true;
    }
  }
  return shown;
}

/**
 * Measures the Task Navigation list and returns how many history rows each fitted group may show.
 * Groups the user expanded with Load More, or collapsed, are not passed in and count as fixed
 * content. Returns an empty map until the list has a measurable layout, so callers fall back to
 * the plain cap; the allocation is presentation-only and never changes App Server demand.
 */
export function useTaskNavigationFit(
  listRef: RefObject<HTMLElement | null>,
  groups: TaskNavigationFitGroup[],
): Map<string, number> {
  const [allocation, setAllocation] = useState<Map<string, number>>(() => new Map());
  const latest = useRef({ allocation, groups });
  latest.current = { allocation, groups };
  const fitRef = useRef<() => void>(undefined);

  // Measuring after every commit catches content changes no observer reports, such as a status
  // banner leaving the list. The allocation converges, so re-measuring settles without a loop.
  useLayoutEffect(() => fitRef.current?.());

  useLayoutEffect(() => {
    const list = listRef.current;
    if (!list || typeof ResizeObserver === "undefined") return undefined;
    const fit = () => {
      const { allocation: rendered, groups: current } = latest.current;
      const viewport = contentBoxHeight(list);
      const first = list.firstElementChild;
      const last = list.lastElementChild;
      if (viewport <= 0 || !first || !last) return;
      const rowPx = list.querySelector<HTMLElement>(".task-row")?.offsetHeight || 32;
      const moreButton = list.querySelector<HTMLElement>(".project-task-more");
      const morePx = moreButton ? outerHeight(moreButton) : rowPx;
      const contentPx = last.getBoundingClientRect().bottom
        + parseFloat(getComputedStyle(last).marginBottom)
        - first.getBoundingClientRect().top;
      // Remove what fitted groups render now, so the result is independent of the last allocation.
      // This mirrors the group's rule: history up to its limit, plus Load More while any is hidden.
      let fittedPx = 0;
      for (const group of current) {
        const shown = Math.min(group.history, rendered.get(group.key) ?? group.cap);
        fittedPx += shown * rowPx + (shown < group.history || group.moreAlways ? morePx : 0);
      }
      const next = allocateHistoryRows(current, viewport - (contentPx - fittedPx), rowPx, morePx);
      setAllocation((previous) => (sameAllocation(previous, next) ? previous : next));
    };
    // The list resizes with the viewport (drawer, keyboard, browser bars); its children resize
    // when rows load, titles wrap, or a collapse animation settles.
    const resizeObserver = new ResizeObserver(fit);
    const observeChildren = () => {
      resizeObserver.disconnect();
      resizeObserver.observe(list);
      for (const child of list.children) resizeObserver.observe(child);
    };
    const childObserver = new MutationObserver(() => {
      observeChildren();
      fit();
    });
    observeChildren();
    childObserver.observe(list, { childList: true });
    fitRef.current = fit;
    fit();
    return () => {
      fitRef.current = undefined;
      resizeObserver.disconnect();
      childObserver.disconnect();
    };
  }, [listRef]);

  return allocation;
}

function contentBoxHeight(element: HTMLElement) {
  const style = getComputedStyle(element);
  return element.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
}

function outerHeight(element: HTMLElement) {
  const style = getComputedStyle(element);
  return element.offsetHeight + parseFloat(style.marginTop) + parseFloat(style.marginBottom);
}

function sameAllocation(left: Map<string, number>, right: Map<string, number>) {
  return left.size === right.size && [...right].every(([key, value]) => left.get(key) === value);
}
