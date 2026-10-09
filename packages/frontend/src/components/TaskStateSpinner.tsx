/**
 * Pins the element's CSS animations to the document timeline origin, so every
 * spinner of one speed shares a phase however late its row mounted.
 */
function pinToDocumentTimeline(element: HTMLElement | null) {
  // jsdom has no Web Animations API, and reduced motion leaves no animation to pin.
  if (!element || typeof element.getAnimations !== "function") return;
  for (const animation of element.getAnimations()) animation.startTime = 0;
}

/** The fast blue arc for running work, or the slow grey dotted ring for background work. */
export function TaskStateSpinner({ slow = false }: { slow?: boolean }) {
  return <span className={slow ? "task-state-background" : "task-state-spinner"} ref={pinToDocumentTimeline} />;
}
