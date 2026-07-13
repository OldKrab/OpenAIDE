import type { ChatMessage, TaskSnapshot } from "@openaide/app-shell-contracts";
import type { ChatPageState } from "./store";
import { coalesceAdjacentActivities } from "./chatActivityCoalescing";
import { visibleNormalizedChatItems } from "./chatItemNormalization";
import { mergeMessageRowsPreferRight, mergePageState } from "./chatPageMerge";

export { mergePageState };

export type RenderedChat = {
  items: ChatMessage[];
  hasBefore: boolean;
  beforeCursor?: string;
  pending: boolean;
  error?: string;
};

export function renderedChat(snapshot: TaskSnapshot, pageState: ChatPageState | undefined): RenderedChat {
  const olderItems = pageState?.olderItems ?? [];
  const items = coalesceAdjacentActivities(
    visibleNormalizedChatItems(mergeMessageRowsPreferRight(olderItems, snapshot.chat.items)),
  );
  const hasBefore = pageState ? pageState.hasBefore : snapshot.chat.has_before;
  return {
    items,
    hasBefore,
    beforeCursor: pageState?.startCursor ?? snapshot.chat.start_cursor,
    pending: pageState?.pending ?? false,
    error: pageState?.error,
  };
}
