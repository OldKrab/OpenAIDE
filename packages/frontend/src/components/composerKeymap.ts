import type { ComposerSubmitShortcut } from "@openaide/app-shell-contracts";

export function shouldSubmitComposerKey(
  event: {
    altKey: boolean;
    ctrlKey: boolean;
    key: string;
    metaKey: boolean;
    nativeEvent?: { isComposing?: boolean };
    shiftKey: boolean;
  },
  submitShortcut: ComposerSubmitShortcut,
) {
  if (event.key !== "Enter" || event.nativeEvent?.isComposing || event.shiftKey || event.altKey) return false;
  const hasCommandModifier = event.ctrlKey || event.metaKey;
  return submitShortcut === "enter" ? !hasCommandModifier : hasCommandModifier;
}

/** Queue is a stable modifier variant of Send, independent of the configured Send shortcut. */
export function shouldQueueComposerKey(event: {
  altKey: boolean;
  ctrlKey: boolean;
  key: string;
  metaKey: boolean;
  nativeEvent?: { isComposing?: boolean };
  shiftKey: boolean;
}) {
  return event.key === "Enter"
    && !event.nativeEvent?.isComposing
    && !event.altKey
    && event.shiftKey
    && (event.ctrlKey || event.metaKey);
}

export function shouldInsertComposerNewline(
  event: {
    altKey: boolean;
    ctrlKey: boolean;
    key: string;
    metaKey: boolean;
    nativeEvent?: { isComposing?: boolean };
    shiftKey: boolean;
  },
  submitShortcut: ComposerSubmitShortcut,
) {
  if (event.key !== "Enter" || event.nativeEvent?.isComposing || event.altKey) return false;
  const hasCommandModifier = event.ctrlKey || event.metaKey;
  return event.shiftKey || (submitShortcut === "enter" && hasCommandModifier);
}
