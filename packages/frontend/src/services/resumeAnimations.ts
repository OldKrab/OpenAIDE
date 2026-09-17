const REARM_MIN_INTERVAL_MS = 5_000;

let lastRearmAt = 0;

/**
 * Rebuilds CSS animation timelines after a runtime suspension.
 *
 * A renderer frozen by machine sleep can stall an element's animation while another
 * element in the same list keeps animating, which is why an in-progress Task spinner
 * can appear stopped next to moving ones. Disabling every animation for one frame and
 * restoring it restarts each timeline from the current time.
 *
 * Throttled so ordinary window focus changes do not restart visible animations.
 */
export function rearmSuspendedAnimations(doc: Document = document, now = Date.now()) {
  if (now - lastRearmAt < REARM_MIN_INTERVAL_MS) return;
  lastRearmAt = now;
  const root = doc.documentElement;
  if (!root) return;
  root.classList.add("oa-animations-suspended");
  // Commit the disabled state before restoring it, or the browser coalesces both
  // changes and no timeline is restarted.
  void root.offsetWidth;
  globalThis.requestAnimationFrame?.(() => root.classList.remove("oa-animations-suspended"));
}
