import type { TaskChatScrollState } from "../state/store";

/** Returns the only programmatic position owned by Chat; readers keep their current viewport. */
export function scrollTopForFollowingViewport({
  clientHeight,
  ownership,
  scrollHeight,
}: {
  clientHeight: number;
  ownership: TaskChatScrollState["ownership"];
  scrollHeight: number;
}) {
  return ownership === "following" ? Math.max(0, scrollHeight - clientHeight) : undefined;
}

export function scrollTopAfterPrependedContent({
  nextScrollHeight,
  previousScrollHeight,
  previousScrollTop,
}: {
  nextScrollHeight: number;
  previousScrollHeight: number;
  previousScrollTop: number;
}) {
  return previousScrollTop + Math.max(0, nextScrollHeight - previousScrollHeight);
}
