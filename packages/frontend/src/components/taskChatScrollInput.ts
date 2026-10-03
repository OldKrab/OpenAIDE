import type { PointerEvent } from "react";

export type ScrollIntent = "towardEarlier" | "towardLatest";

/** React input handlers can run before the virtualizer observes this scroll. */
export function isChatViewportAtEnd(viewport: HTMLDivElement | null) {
  return viewport !== null && viewport.scrollHeight - viewport.clientHeight - viewport.scrollTop <= 2;
}

export function keyboardScrollDirection(key: string, shiftKey: boolean): ScrollIntent | undefined {
  if (key === "PageUp" || key === "Home" || key === "ArrowUp" || (key === " " && shiftKey)) {
    return "towardEarlier";
  }
  if (key === "PageDown" || key === "End" || key === "ArrowDown" || (key === " " && !shiftKey)) {
    return "towardLatest";
  }
  return undefined;
}

/** Nested editors and controls own their keys instead of navigating Chat. */
export function nestedControlOwnsScrollKey(target: EventTarget, viewport: HTMLDivElement) {
  if (target === viewport) return false;
  const closest = (target as { closest?: (selector: string) => Element | null }).closest;
  if (typeof closest !== "function") return true;
  return Boolean(closest.call(
    target,
    "a[href], button, input, select, summary, textarea, [contenteditable='true'], [role='listbox'], [role='slider']",
  ));
}

/** Overlay scrollbars have no layout width, but still accept edge dragging. */
export function isVerticalScrollbarPointer(event: PointerEvent<HTMLDivElement>) {
  if (event.pointerType !== "mouse") return false;
  if (event.currentTarget.scrollHeight <= event.currentTarget.clientHeight) return false;
  const scrollbarWidth = Math.max(
    event.currentTarget.offsetWidth - event.currentTarget.clientWidth,
    10,
  );
  const bounds = event.currentTarget.getBoundingClientRect();
  return event.clientX >= bounds.right - scrollbarWidth && event.clientX <= bounds.right;
}
