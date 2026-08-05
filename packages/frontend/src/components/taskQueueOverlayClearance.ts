const QUEUE_ANCHOR_HEIGHT = 34;
const CLEARANCE_PROPERTY = "--task-queue-overlay-clearance";

/** Keeps the floating Queue from covering the final Chat rows without resizing Chat itself. */
export function installTaskQueueOverlayClearance(floating: HTMLDivElement): () => void {
  const conversation = floating.closest<HTMLElement>(".task-conversation");
  if (!conversation) return () => undefined;

  const update = () => {
    const expandedHeight = Math.max(0, floating.getBoundingClientRect().height - QUEUE_ANCHOR_HEIGHT);
    conversation.style.setProperty(CLEARANCE_PROPERTY, `${Math.round(expandedHeight)}px`);
  };
  update();

  const observer = typeof ResizeObserver === "function" ? new ResizeObserver(update) : undefined;
  observer?.observe(floating);
  return () => {
    observer?.disconnect();
    conversation.style.removeProperty(CLEARANCE_PROPERTY);
  };
}
