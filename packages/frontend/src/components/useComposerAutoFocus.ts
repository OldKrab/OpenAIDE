import { useEffect, type RefObject } from "react";
import type { ComposerEditorHandle } from "./ComposerEditor";

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
    if (!autoFocus || disabled || hasCoarsePointer()) return;
    editorRef.current?.focus();
  }, [autoFocus, disabled, editorRef, focusRequestKey]);
}

function hasCoarsePointer() {
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia("(hover: none) and (pointer: coarse)").matches;
}
