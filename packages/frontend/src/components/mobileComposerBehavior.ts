import type { MouseEvent as ReactMouseEvent } from "react";

const MOBILE_COMPOSER_QUERY = "(hover: none) and (pointer: coarse)";
const TEXT_ENTRY_SELECTOR = "input, select, textarea, [contenteditable]:not([contenteditable='false'])";
const CONTROL_SELECTOR = "a, button, label, summary, [role='button'], [role='menuitem'], [role='option'], [role='switch']";

/** Mobile software keyboards use Return for new lines and the visible button for sending. */
export function usesMobileComposerBehavior() {
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia(MOBILE_COMPOSER_QUERY).matches;
}

/**
 * Android closes the software keyboard when the focused text field loses focus, which a tap does
 * on its mousedown. Cancelling only that default keeps the keyboard up and still delivers the click.
 * Touch scrolling raises no mouse events. A tap on another text field moves focus as usual.
 */
export function keepTextEntryFocusOnTap(event: ReactMouseEvent<HTMLElement>) {
  if (event.button !== 0 || !usesMobileComposerBehavior()) return;
  if (!event.currentTarget.ownerDocument.activeElement?.matches(TEXT_ENTRY_SELECTOR)) return;
  if (event.target instanceof Element && event.target.closest(TEXT_ENTRY_SELECTOR)) return;
  event.preventDefault();
}

/**
 * The same for a tap on the blank space around the Composer. Controls stay out of it: Send and
 * Composer menus rely on the focus change they make, and Stop opts in on its own.
 */
export function keepTextEntryFocusOnBlankTap(event: ReactMouseEvent<HTMLElement>) {
  if (event.target instanceof Element && event.target.closest(CONTROL_SELECTOR)) return;
  keepTextEntryFocusOnTap(event);
}

/** The app shell's event for Android's software keyboard closing; browsers do not report it. */
export const KEYBOARD_HIDDEN_EVENT = "openaide:keyboardHidden";

/**
 * Android hides the software keyboard on Back or its own hide key but leaves the text field
 * focused, and Chrome raises the keyboard again on any later tap while one is, so a tap on Chat
 * would reopen it. The app shell reports the keyboard closing from Android's own IME state, and
 * the field lets go. Viewport size is no substitute: it also changes while the keyboard opens on
 * some phones. Returns the cleanup.
 */
export function releaseTextEntryWhenKeyboardHides(host: Window | undefined = globalThis.window) {
  if (!host) return () => {};
  const hidden = () => {
    const active = host.document.activeElement as HTMLElement | null;
    if (active?.matches(TEXT_ENTRY_SELECTOR)) active.blur();
  };
  host.addEventListener(KEYBOARD_HIDDEN_EVENT, hidden);
  return () => host.removeEventListener(KEYBOARD_HIDDEN_EVENT, hidden);
}
