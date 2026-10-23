/**
 * Calls [resumed] when the page returns to the screen, the machine resumes, or
 * the network comes back: the moments a stalled connection is worth trying again.
 */
export function subscribeToBrowserResume(resumed: () => void) {
  if (typeof document === "undefined" || typeof window === "undefined") return () => undefined;
  let wasHidden = document.visibilityState === "hidden";
  const handleVisibilityChange = () => {
    const hidden = document.visibilityState === "hidden";
    if (wasHidden && !hidden) resumed();
    wasHidden = hidden;
  };
  const handlePageShow = () => resumed();
  const handleOnline = () => resumed();
  // Focus covers a machine resume where the window was never hidden.
  const handleFocus = () => resumed();
  document.addEventListener?.("visibilitychange", handleVisibilityChange);
  window.addEventListener?.("pageshow", handlePageShow);
  window.addEventListener?.("online", handleOnline);
  window.addEventListener?.("focus", handleFocus);
  window.addEventListener?.("openaide:resume", handlePageShow);
  return () => {
    document.removeEventListener?.("visibilitychange", handleVisibilityChange);
    window.removeEventListener?.("pageshow", handlePageShow);
    window.removeEventListener?.("online", handleOnline);
    window.removeEventListener?.("focus", handleFocus);
    window.removeEventListener?.("openaide:resume", handlePageShow);
  };
}
