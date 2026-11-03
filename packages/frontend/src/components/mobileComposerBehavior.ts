import type { MouseEvent as ReactMouseEvent } from "react";

const MOBILE_COMPOSER_QUERY = "(hover: none) and (pointer: coarse)";
const TEXT_ENTRY_SELECTOR = "input, select, textarea, [contenteditable]:not([contenteditable='false'])";
const CONTROL_SELECTOR = "a, button, label, summary, [role='button'], [role='menuitem'], [role='option'], [role='switch']";
// A software keyboard is far taller than a browser toolbar that collapses while scrolling.
const KEYBOARD_CLOSE_GROWTH_PX = 120;

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

/**
 * Android hides the software keyboard on Back or its own hide key but leaves the text field
 * focused, and Chrome raises the keyboard again on any later tap while one is, so a tap on Chat
 * would reopen it. The visible viewport growing by a keyboard's height while a text field holds
 * focus means the keyboard closed, and the field lets go. Returns the cleanup.
 */
export function releaseTextEntryWhenKeyboardCloses(target: Window | undefined = globalThis.window) {
  const viewport = target?.visualViewport;
  if (!target || !viewport || !usesMobileComposerBehavior()) return () => {};
  const document = target.document;
  let lowest = viewport.height;
  const resized = () => {
    const active = document.activeElement;
    const typing = active instanceof HTMLElement && active.matches(TEXT_ENTRY_SELECTOR);
    lowest = typing ? Math.min(lowest, viewport.height) : viewport.height;
    if (!typing || viewport.height - lowest < KEYBOARD_CLOSE_GROWTH_PX) return;
    lowest = viewport.height;
    active.blur();
  };
  viewport.addEventListener("resize", resized);
  return () => viewport.removeEventListener("resize", resized);
}
