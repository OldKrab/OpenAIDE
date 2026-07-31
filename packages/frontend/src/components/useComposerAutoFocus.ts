import { useEffect, type RefObject } from "react";
import type { ComposerEditorHandle } from "./ComposerEditor";
import { usesMobileComposerBehavior } from "./mobileComposerBehavior";

type ComposerAutoFocusOptions = {
  autoFocus: boolean;
  disabled: boolean;
  editorRef: RefObject<ComposerEditorHandle | null>;
  focusRequestKey?: number | string;
};

/** Keeps keyboard flow on desktop without summoning a touch keyboard on mobile. */
export function useComposerAutoFocus({
  autoFocus,
  disabled,
  editorRef,
  focusRequestKey,
}: ComposerAutoFocusOptions) {
  useEffect(() => {
    if (!autoFocus || disabled || usesMobileComposerBehavior()) return;
    const focusEditor = () => editorRef.current?.focus();
    focusEditor();

    if (typeof window === "undefined" || typeof window.addEventListener !== "function") return;
    const restoreDroppedFocus = () => {
      // VS Code can return a retained webview with focus on its document body.
      // Preserve any real control focus instead of always stealing it back.
      if (document.activeElement === document.body) focusEditor();
    };
    window.addEventListener("focus", restoreDroppedFocus);
    return () => window.removeEventListener("focus", restoreDroppedFocus);
  }, [autoFocus, disabled, editorRef, focusRequestKey]);
}
