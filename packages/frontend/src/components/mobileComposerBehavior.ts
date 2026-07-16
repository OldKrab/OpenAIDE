const MOBILE_COMPOSER_QUERY = "(hover: none) and (pointer: coarse)";

/** Mobile software keyboards use Return for new lines and the visible button for sending. */
export function usesMobileComposerBehavior() {
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia(MOBILE_COMPOSER_QUERY).matches;
}
