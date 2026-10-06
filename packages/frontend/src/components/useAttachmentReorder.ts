import { useLayoutEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type PointerEvent } from "react";

const DRAG_THRESHOLD_PX = 6;
const EDGE_SCROLL_ZONE_PX = 12;
const EDGE_SCROLL_STEP_PX = 8;
const LAYOUT_ANIMATION_ID = "attachment-layout";

type ReorderItem = { id: string; label: string };
type Slot = { left: number; top: number };

/**
 * Owns the ephemeral drag preview for the Composer attachment list. The draft
 * order itself stays with the caller: `onMove` fires once per completed move.
 *
 * Tiles wrap in two dimensions, so the preview reorders the rendered list while
 * the pointer crosses other tiles instead of projecting a one-axis insertion slot.
 */
export function useAttachmentReorder({
  enabled,
  items,
  onMove,
}: {
  enabled: boolean;
  items: ReorderItem[];
  onMove?: (id: string, targetIndex: number) => void;
}) {
  const listRef = useRef<HTMLDivElement>(null);
  const [preview, setPreview] = useState<{ id: string; index: number }>();
  const [announcement, setAnnouncement] = useState("");
  const drag = useRef<{
    id: string;
    pointerId: number;
    startX: number;
    startY: number;
    grabX: number;
    grabY: number;
    x: number;
    y: number;
    armed: boolean;
  } | undefined>(undefined);
  const previousLayout = useRef<{ order: string; slots: Map<string, Slot> } | undefined>(undefined);
  const suppressClick = useRef(false);
  const refocusId = useRef<string | undefined>(undefined);

  const ids = items.map((item) => item.id);
  const reorderable = enabled && onMove !== undefined && items.length > 1;
  const orderedIds = preview && ids.includes(preview.id) ? moveId(ids, preview.id, preview.index) : ids;

  const tiles = () => Array.from(listRef.current?.querySelectorAll<HTMLElement>("[data-attachment-id]") ?? []);
  const tileFor = (id: string) => tiles().find((tile) => tile.dataset.attachmentId === id);

  /**
   * Pins the dragged tile under the pointer, clamped so it never widens the scrollable list.
   * Returns the tile's visible center in list coordinates: the drop position follows what
   * the user sees, so it stays the same whether the pointer is inside or outside the list.
   */
  const followPointer = () => {
    const current = drag.current;
    const list = listRef.current;
    const tile = current?.armed ? tileFor(current.id) : undefined;
    if (!current || !list || !tile) return undefined;
    const bounds = list.getBoundingClientRect();
    const left = clamp(current.x - bounds.left - current.grabX, 0, list.offsetWidth - tile.offsetWidth);
    const top = clamp(current.y - bounds.top - current.grabY, 0, list.offsetHeight - tile.offsetHeight);
    tile.style.transform = `translate3d(${left - tile.offsetLeft}px, ${top - tile.offsetTop}px, 0)`;
    return { x: left + tile.offsetWidth / 2, y: top + tile.offsetHeight / 2 };
  };

  // Offsets ignore transforms, so slots stay exact while tiles animate or follow the pointer.
  useLayoutEffect(() => {
    const rendered = tiles();
    const order = rendered.map((tile) => tile.dataset.attachmentId).join("\u0000");
    const slots = new Map(rendered.map((tile) => [
      tile.dataset.attachmentId!,
      { left: tile.offsetLeft, top: tile.offsetTop },
    ]));
    const previous = previousLayout.current;
    previousLayout.current = { order, slots };
    followPointer();
    if (refocusId.current) {
      // Reordering may reinsert the focused tile; keep keyboard moves chainable.
      tileFor(refocusId.current)?.querySelector<HTMLElement>("[data-attachment-grip]")?.focus();
      refocusId.current = undefined;
    }
    if (!previous || previous.order === order) return;
    if (globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;
    for (const tile of rendered) {
      const id = tile.dataset.attachmentId!;
      const from = previous.slots.get(id);
      const to = slots.get(id)!;
      if (!from || (drag.current?.armed && drag.current.id === id)) continue;
      // Continue from the visible position when an earlier move is still settling.
      const running = tile.getAnimations?.().filter((animation) => animation.id === LAYOUT_ANIMATION_ID) ?? [];
      let offsetX = from.left - to.left;
      let offsetY = from.top - to.top;
      if (running.length > 0) {
        const visible = tile.getBoundingClientRect();
        for (const animation of running) animation.cancel();
        const settled = tile.getBoundingClientRect();
        offsetX += visible.left - settled.left;
        offsetY += visible.top - settled.top;
      }
      if (Math.abs(offsetX) < 0.5 && Math.abs(offsetY) < 0.5) continue;
      animateSettle(tile, offsetX, offsetY);
    }
  });

  const commit = (id: string, targetIndex: number) => {
    const fromIndex = ids.indexOf(id);
    const clampedIndex = clamp(targetIndex, 0, ids.length - 1);
    if (!reorderable || fromIndex < 0 || clampedIndex === fromIndex) return false;
    onMove?.(id, clampedIndex);
    setAnnouncement(`${items[fromIndex]!.label} moved to position ${clampedIndex + 1} of ${ids.length}`);
    return true;
  };

  const beginDrag = (event: PointerEvent<HTMLElement>, id: string, fromGrip: boolean) => {
    if (!reorderable || event.button > 0) return;
    // Touch on the tile body stays with native scrolling; the grip owns touch reordering.
    if (event.pointerType === "touch" && !fromGrip) return;
    const bounds = tileFor(id)?.getBoundingClientRect();
    drag.current = {
      id,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      grabX: event.clientX - (bounds?.left ?? event.clientX),
      grabY: event.clientY - (bounds?.top ?? event.clientY),
      x: event.clientX,
      y: event.clientY,
      armed: false,
    };
  };

  const updateDrag = (event: PointerEvent<HTMLElement>) => {
    const current = drag.current;
    if (!current || current.pointerId !== event.pointerId) return;
    if (!current.armed) {
      if (Math.hypot(event.clientX - current.startX, event.clientY - current.startY) < DRAG_THRESHOLD_PX) return;
      current.armed = true;
      // Capture only once dragging starts so plain clicks still reach the tile's own buttons.
      listRef.current?.setPointerCapture?.(event.pointerId);
    }
    current.x = event.clientX;
    current.y = event.clientY;
    event.preventDefault();

    const scroller = listRef.current?.parentElement;
    if (scroller) {
      const scrollBounds = scroller.getBoundingClientRect();
      if (current.y < scrollBounds.top + EDGE_SCROLL_ZONE_PX) scroller.scrollTop -= EDGE_SCROLL_STEP_PX;
      else if (current.y > scrollBounds.bottom - EDGE_SCROLL_ZONE_PX) scroller.scrollTop += EDGE_SCROLL_STEP_PX;
    }
    let index = orderedIds.indexOf(current.id);
    const center = followPointer();
    if (center) {
      const over = tiles().find((tile) => (
        tile.dataset.attachmentId !== current.id
        && center.x >= tile.offsetLeft && center.x < tile.offsetLeft + tile.offsetWidth
        && center.y >= tile.offsetTop && center.y < tile.offsetTop + tile.offsetHeight
      ));
      if (over) index = orderedIds.indexOf(over.dataset.attachmentId!);
    }
    if (index >= 0 && (preview?.id !== current.id || preview.index !== index)) {
      setPreview({ id: current.id, index });
    }
  };

  const finishDrag = (event: PointerEvent<HTMLElement>, cancelled: boolean) => {
    const current = drag.current;
    if (!current || current.pointerId !== event.pointerId) return;
    drag.current = undefined;
    if (!current.armed) return;
    // The release that ends a drag must not also open an Image or trigger a tile action.
    suppressClick.current = true;
    setTimeout(() => { suppressClick.current = false; }, 0);
    const tile = tileFor(current.id);
    if (tile) {
      const dropped = tile.getBoundingClientRect();
      tile.style.transform = "";
      const settled = tile.getBoundingClientRect();
      if (!cancelled && !globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches) {
        animateSettle(tile, dropped.left - settled.left, dropped.top - settled.top);
      }
    }
    if (!cancelled && preview?.id === current.id) commit(current.id, preview.index);
    setPreview(undefined);
  };

  const moveWithKeyboard = (event: KeyboardEvent<HTMLElement>, id: string) => {
    const delta = event.key === "ArrowLeft" || event.key === "ArrowUp"
      ? -1
      : event.key === "ArrowRight" || event.key === "ArrowDown" ? 1 : 0;
    if (!delta) return;
    event.preventDefault();
    if (commit(id, ids.indexOf(id) + delta)) refocusId.current = id;
  };

  return {
    announcement,
    draggingId: preview?.id,
    orderedIds,
    reorderable,
    listProps: {
      ref: listRef,
      onClickCapture: (event: MouseEvent<HTMLElement>) => {
        if (!suppressClick.current) return;
        suppressClick.current = false;
        event.preventDefault();
        event.stopPropagation();
      },
      onPointerMove: updateDrag,
      onPointerUp: (event: PointerEvent<HTMLElement>) => finishDrag(event, false),
      onPointerCancel: (event: PointerEvent<HTMLElement>) => finishDrag(event, true),
    },
    tileProps: (id: string) => ({
      "data-attachment-id": id,
      "data-dragging": preview?.id === id ? true : undefined,
      onPointerDown: reorderable ? (event: PointerEvent<HTMLElement>) => beginDrag(event, id, false) : undefined,
    }),
    gripProps: (id: string) => ({
      "data-attachment-grip": true,
      onKeyDown: (event: KeyboardEvent<HTMLElement>) => moveWithKeyboard(event, id),
      onPointerDown: (event: PointerEvent<HTMLElement>) => {
        event.stopPropagation();
        beginDrag(event, id, true);
      },
    }),
  };
}

function animateSettle(tile: HTMLElement, offsetX: number, offsetY: number) {
  if (typeof tile.animate !== "function") return;
  const animation = tile.animate([
    { transform: `translate3d(${offsetX}px, ${offsetY}px, 0)` },
    { transform: "translate3d(0, 0, 0)" },
  ], { duration: 180, easing: "cubic-bezier(.16, 1, .3, 1)" });
  animation.id = LAYOUT_ANIMATION_ID;
}

function moveId(ids: string[], id: string, targetIndex: number) {
  const reordered = ids.filter((candidate) => candidate !== id);
  reordered.splice(clamp(targetIndex, 0, reordered.length), 0, id);
  return reordered;
}

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(value, Math.max(min, max)));
}
