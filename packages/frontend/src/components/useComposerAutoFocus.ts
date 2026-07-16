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
    editorRef.current?.focus();
  }, [autoFocus, disabled, editorRef, focusRequestKey]);
}
