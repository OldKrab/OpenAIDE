import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent } from "react";
import { ChevronDown, GripVertical, ListPlus, LoaderCircle, Pencil, SendHorizontal, Trash2 } from "lucide-react";
import type { TaskMessageQueue } from "@openaide/app-shell-contracts";
import { IconButton } from "./ComposerPrimitives";

/** Renders durable follow-ups separately from Chat until the App Server sends them. */
export function TaskMessageQueueView({
  editDisabled = false,
  extractionStage = "pending",
  extractingQueuedMessageId,
  queue,
  onTake,
  onMove,
  onExpanded,
  onRemove,
  onSendNow,
}: {
  editDisabled?: boolean;
  extractionStage?: "pending" | "collapsing";
  extractingQueuedMessageId?: string;
  queue: TaskMessageQueue;
  onTake?: (queuedMessageId: string) => void;
  onMove?: (queuedMessageId: string, targetIndex: number) => void | Promise<void>;
  onExpanded?: () => void;
  onRemove: (queuedMessageId: string) => void;
  onSendNow?: (queuedMessageId: string) => void;
}) {
  const [open, setOpen] = useState(true);
  const [draggingId, setDraggingId] = useState<string>();
  const [dropIndex, setDropIndex] = useState<number>();
  const [dragOffsetY, setDragOffsetY] = useState(0);
  const [dragSlotHeight, setDragSlotHeight] = useState(0);
  const [optimisticOrder, setOptimisticOrder] = useState<string[]>();
  const drag = useRef<{ id: string; pointerId: number; startY: number; armed: boolean } | undefined>(undefined);
  const listRef = useRef<HTMLOListElement>(null);
  const previousLayout = useRef<{
    dragging: boolean;
    movedId?: string;
    order: string;
    rowTops: Map<string, number>;
  } | undefined>(undefined);
  const items = useMemo(() => {
    if (!optimisticOrder) return queue.items;
    const itemsById = new Map(queue.items.map((item) => [item.queued_message_id, item]));
    const ordered = optimisticOrder.flatMap((id) => {
      const item = itemsById.get(id);
      if (!item) return [];
      itemsById.delete(id);
      return [item];
    });
    return [...ordered, ...itemsById.values()];
  }, [optimisticOrder, queue.items]);
  useEffect(() => {
    if (!optimisticOrder) return;
    const durableOrder = queue.items.map((item) => item.queued_message_id);
    if (durableOrder.length === optimisticOrder.length
      && durableOrder.every((id, index) => id === optimisticOrder[index])) {
      setOptimisticOrder(undefined);
    }
  }, [optimisticOrder, queue.items]);
  const single = items.length === 1;
  const hasReorderControl = !single && onMove !== undefined;
  const reorderable = hasReorderControl;

  // Leave Chat follow mode before the floating rows can change its scroll range.
  useLayoutEffect(() => {
    if (!single && open) onExpanded?.();
  }, [onExpanded, open, single]);

  // Preserve visual positions across both an immediate prototype reorder and an async server response.
  useLayoutEffect(() => {
    const rows = Array.from(listRef.current?.querySelectorAll<HTMLElement>("li[data-queue-row]") ?? []);
    if (rows.length === 0) {
      previousLayout.current = undefined;
      return;
    }
    const order = rows.map((row) => row.dataset.queueId).join("\u0000");
    const previous = previousLayout.current;
    const shouldSettle = previous?.dragging && !draggingId;
    const orderChanged = previous !== undefined && previous.order !== order;
    const animateLayout = (shouldSettle || orderChanged)
      && !globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    if (animateLayout) {
      for (const row of rows) {
        for (const animation of row.getAnimations()) {
          if (animation.id === "queue-layout") animation.cancel();
        }
      }
    }
    const rowTops = new Map(rows.map((row) => [row.dataset.queueId!, row.getBoundingClientRect().top]));

    if (animateLayout) {
      for (const row of rows) {
        const previousTop = previous?.rowTops.get(row.dataset.queueId!);
        if (previousTop === undefined) continue;
        const offset = previousTop - row.getBoundingClientRect().top;
        if (Math.abs(offset) < 0.5) continue;
        const moved = row.dataset.queueId === previous?.movedId;
        const animation = row.animate([
          {
            transform: `translate3d(0, ${offset}px, 0) scale(${moved ? 1.01 : 1})`,
            opacity: moved ? 0.96 : 1,
          },
          { transform: "translate3d(0, 0, 0) scale(1)", opacity: 1 },
        ], {
          duration: 200,
          easing: "cubic-bezier(.16, 1, .3, 1)",
        });
        animation.id = "queue-layout";
      }
    }

    previousLayout.current = { dragging: Boolean(draggingId), movedId: draggingId, order, rowTops };
  });

  if (items.length === 0) return null;

  const beginDrag = (event: PointerEvent, id: string, fromGrip: boolean) => {
    if (event.pointerType === "touch" && !fromGrip) return;
    // Reordering owns this pointer; block browser text/native dragging before the movement threshold.
    event.preventDefault();
    drag.current = { id, pointerId: event.pointerId, startY: event.clientY, armed: false };
    event.currentTarget.setPointerCapture?.(event.pointerId);
  };
  const updateDrag = (event: PointerEvent) => {
    const current = drag.current;
    if (!current || current.pointerId !== event.pointerId) return;
    if (!current.armed && Math.abs(event.clientY - current.startY) < 7) return;
    if (!current.armed) {
      current.armed = true;
      const rows = Array.from(event.currentTarget.querySelectorAll<HTMLElement>("li[data-queue-row]"));
      const draggedRow = rows.find((row) => row.dataset.queueId === current.id);
      const rowGap = listRef.current
        ? Number.parseFloat(globalThis.getComputedStyle(listRef.current).rowGap) || 0
        : 7;
      setDragSlotHeight((draggedRow?.offsetHeight ?? 44) + rowGap);
    }
    setDraggingId(current.id);
    const nextDragOffsetY = event.clientY - current.startY;
    setDragOffsetY(nextDragOffsetY);
    const rows = Array.from(event.currentTarget.querySelectorAll<HTMLElement>("li[data-queue-row]"));
    const listBounds = typeof event.currentTarget.getBoundingClientRect === "function"
      ? event.currentTarget.getBoundingClientRect()
      : undefined;
    const listScrollTop = event.currentTarget.scrollTop ?? 0;
    const layoutTop = (row: HTMLElement) => (
      listBounds && Number.isFinite(row.offsetTop)
        ? listBounds.top + row.offsetTop - listScrollTop
        : row.getBoundingClientRect().top
    );
    const draggedRow = rows.find((row) => row.dataset.queueId === current.id);
    const draggedCenterY = draggedRow
      ? layoutTop(draggedRow) + draggedRow.offsetHeight / 2 + nextDragOffsetY
      : event.clientY;
    const nextIndex = rows.filter((row) => row.dataset.queueId !== current.id)
      .findIndex((row) => draggedCenterY < layoutTop(row) + row.offsetHeight / 2);
    setDropIndex(nextIndex < 0 ? rows.length - 1 : nextIndex);
    event.preventDefault();
  };
  const requestMove = (id: string, targetIndex: number) => {
    setOptimisticOrder(moveQueueItem(items, id, targetIndex).map((item) => item.queued_message_id));
    const result = onMove?.(id, targetIndex);
    if (result) void result.catch(() => setOptimisticOrder(undefined));
  };
  const finishDrag = (event: PointerEvent) => {
    const current = drag.current;
    if (!current || current.pointerId !== event.pointerId) return;
    if (current.armed && dropIndex !== undefined) {
      requestMove(current.id, dropIndex);
    }
    drag.current = undefined;
    setDraggingId(undefined);
    setDropIndex(undefined);
    setDragOffsetY(0);
    setDragSlotHeight(0);
  };
  const moveWithKeyboard = (event: KeyboardEvent, id: string, index: number) => {
    const delta = event.key === "ArrowUp" ? -1 : event.key === "ArrowDown" ? 1 : 0;
    if (!delta) return;
    event.preventDefault();
    const targetIndex = Math.max(0, Math.min(items.length - 1, index + delta));
    requestMove(id, targetIndex);
  };

  return (
    <section
      aria-label={single ? "Queued message" : "Message queue"}
      className="task-message-queue"
      data-open={single || open}
      data-single={single ? true : undefined}
    >
      <div
        aria-hidden={single ? false : !open}
        className="task-message-queue-disclosure"
        data-open={single || open}
        inert={single || open ? undefined : true}
      >
        <div className="task-message-queue-content">
        <ol
          ref={listRef}
          className="task-message-queue-items"
          data-dragging={draggingId ? true : undefined}
          data-reorderable={reorderable ? true : undefined}
          onPointerMove={updateDrag}
          onPointerUp={finishDrag}
          onPointerCancel={finishDrag}
        >
          {items.map((item, index) => {
            const draggedIndex = draggingId
              ? items.findIndex((candidate) => candidate.queued_message_id === draggingId)
              : -1;
            const reorderedRows = items.filter((candidate) => candidate.queued_message_id !== draggingId);
            const visibleIndex = reorderedRows.findIndex((candidate) => candidate.queued_message_id === item.queued_message_id);
            const isLastVisible = visibleIndex === reorderedRows.length - 1;
            const extracting = extractingQueuedMessageId === item.queued_message_id;
            const dropBefore = Boolean(draggingId && visibleIndex >= 0 && dropIndex === visibleIndex);
            const dropAfter = Boolean(
              draggingId
              && visibleIndex >= 0
              && isLastVisible
              && dropIndex === reorderedRows.length,
            );
            const shiftY = draggingId && dropIndex !== undefined
              ? draggedIndex < dropIndex && index > draggedIndex && index <= dropIndex
                ? -dragSlotHeight
                : draggedIndex > dropIndex && index >= dropIndex && index < draggedIndex
                  ? dragSlotHeight
                  : 0
              : 0;
            const rowStyle = draggingId === item.queued_message_id || shiftY !== 0
              ? {
                  "--queue-drag-y": draggingId === item.queued_message_id ? `${dragOffsetY}px` : "0px",
                  "--queue-shift-y": `${shiftY}px`,
                } as CSSProperties
              : undefined;
            return (
            <li
              aria-busy={extracting || undefined}
              data-drop-before={dropBefore || undefined}
              data-drop-after={dropAfter || undefined}
              data-dragging={draggingId === item.queued_message_id ? true : undefined}
              data-extracting={extracting ? extractionStage : undefined}
              data-queue-id={item.queued_message_id}
              data-queue-row
              key={item.queued_message_id}
              onPointerDown={reorderable && !extracting
                ? (event) => beginDrag(event, item.queued_message_id, false)
                : undefined}
              style={rowStyle}
            >
              {dropBefore || dropAfter ? (
                <span
                  aria-hidden="true"
                  className="task-message-queue-drop-marker"
                  data-position={dropBefore ? "before" : "after"}
                />
              ) : null}
              {hasReorderControl ? (
                <button
                  aria-label={`Reorder ${item.text}`}
                  className="task-message-queue-grip"
                  disabled={extracting}
                  onKeyDown={(event) => moveWithKeyboard(event, item.queued_message_id, index)}
                  onPointerDown={extracting ? undefined : (event) => {
                    event.stopPropagation();
                    beginDrag(event, item.queued_message_id, true);
                  }}
                  type="button"
                ><GripVertical aria-hidden="true" size={14} /></button>
              ) : <ListPlus aria-hidden="true" className="task-message-queue-single-icon" size={14} />}
              <div className="task-message-queue-copy">
                <span>{item.text || "Attachment-only message"}</span>
                {item.attachments?.length ? (
                  <small>{item.attachments.map((attachment) => attachment.label).join(", ")}</small>
                ) : null}
              </div>
              <div className="task-message-queue-actions" onPointerDown={(event) => event.stopPropagation()}>
                <IconButton
                  ariaLabel="Send queued message now"
                  disabled={extracting}
                  icon={<SendHorizontal size={14} />}
                  onClick={() => onSendNow?.(item.queued_message_id)}
                />
                <IconButton
                  ariaLabel={extracting ? "Moving queued message to Composer" : "Edit in Composer"}
                  disabled={extracting || editDisabled}
                  icon={extracting
                    ? <LoaderCircle aria-hidden="true" className="task-message-queue-pending" size={14} />
                    : <Pencil size={14} />}
                  onClick={() => onTake?.(item.queued_message_id)}
                />
                <IconButton
                  ariaLabel="Delete queued message"
                  disabled={extracting}
                  icon={<Trash2 size={14} />}
                  onClick={() => onRemove(item.queued_message_id)}
                />
              </div>
            </li>
          );})}
        </ol>
        {queue.pause ? (
          <div className="task-message-queue-pause" role="status">
            <span>{queue.pause === "restarted"
              ? "Paused after restart"
              : queue.pause === "attachmentUnavailable"
                ? "Paused: attachment unavailable"
                : "Paused after interrupted work"}</span>
            <button onClick={() => onSendNow?.(queue.items[0]!.queued_message_id)} type="button">
              Resume queue
            </button>
          </div>
        ) : null}
        </div>
      </div>
      {!single ? <button
        aria-expanded={open}
        aria-label={`${open ? "Collapse" : "Expand"} message queue`}
        className="task-message-queue-heading"
        onClick={() => setOpen((value) => !value)}
        type="button"
      >
        <ChevronDown aria-hidden="true" size={15} />
        <strong>Queue</strong>
        {!open ? <span>{queue.items[0]?.text}</span> : null}
        <small>{queue.items.length}</small>
      </button> : null}
    </section>
  );
}

/** Reorders one durable queue identity without mutating the authoritative snapshot. */
function moveQueueItem<T extends { queued_message_id: string }>(items: T[], id: string, targetIndex: number) {
  const fromIndex = items.findIndex((item) => item.queued_message_id === id);
  if (fromIndex < 0) return items;
  const reordered = [...items];
  const [item] = reordered.splice(fromIndex, 1);
  reordered.splice(Math.max(0, Math.min(targetIndex, reordered.length)), 0, item);
  return reordered;
}
