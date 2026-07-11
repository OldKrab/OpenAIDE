import { useCallback, useEffect, useState } from "react";

/** Tracks input modality because text editors match :focus-visible after pointer focus in Chromium. */
export function useComposerKeyboardFocus() {
  const [keyboardFocus, setKeyboardFocus] = useState(false);
  const onKeyboardNavigation = useCallback((event: { key: string }) => {
    if (event.key === "Tab") setKeyboardFocus(true);
  }, []);
  const onPointerInteraction = useCallback(() => setKeyboardFocus(false), []);

  useEffect(() => {
    if (typeof document === "undefined") return undefined;
    document.addEventListener("keydown", onKeyboardNavigation, true);
    document.addEventListener("pointerdown", onPointerInteraction, true);
    return () => {
      document.removeEventListener("keydown", onKeyboardNavigation, true);
      document.removeEventListener("pointerdown", onPointerInteraction, true);
    };
  }, [onKeyboardNavigation, onPointerInteraction]);

  return { keyboardFocus, onKeyboardNavigation, onPointerInteraction };
}
