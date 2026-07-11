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
