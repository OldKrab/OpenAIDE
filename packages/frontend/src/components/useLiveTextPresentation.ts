import { useEffect, useLayoutEffect, useMemo, useState } from "react";
import {
  advanceLiveText,
  receiveLiveText,
  stableMarkdownTarget,
  startLiveText,
} from "./liveTextPresentation";

export function useLiveTextPresentation(text: string, streaming: boolean, forceImmediate = false) {
  const target = useMemo(
    () => streaming ? stableMarkdownTarget(text) : text,
    [streaming, text],
  );
  const immediate = forceImmediate || presentationShouldBeImmediate();
  const [presentation, setPresentation] = useState(() => (
    streaming && !immediate
      ? receiveLiveText(startLiveText(""), target)
      : startLiveText(target)
  ));

  useLayoutEffect(() => {
    setPresentation((current) => immediate
      ? startLiveText(target)
      : receiveLiveText(current, target));
  }, [immediate, target]);

  useEffect(() => {
    if (presentation.caughtUp || immediate) return;
    const frame = requestAnimationFrame(() => {
      setPresentation((current) => advanceLiveText(current));
    });
    return () => cancelAnimationFrame(frame);
  }, [immediate, presentation]);

  return {
    caughtUp: presentation.caughtUp && presentation.receivedText === text,
    text: presentation.visibleText,
  };
}

function presentationShouldBeImmediate() {
  if (typeof document !== "undefined" && document.visibilityState === "hidden") return true;
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return false;
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}
