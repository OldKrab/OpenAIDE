import type { ChatMessage, TaskSnapshot } from "@openaide/app-shell-contracts";
import type { ChatPageState } from "./store";
import { coalesceAdjacentActivities } from "./chatActivityCoalescing";
import { visibleNormalizedChatItems } from "./chatItemNormalization";
import { mergeMessageRows, mergePageState } from "./chatPageMerge";
import { coalesceAdjacentAgentText, coalesceAdjacentThoughts } from "./chatTextCoalescing";

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
    coalesceAdjacentThoughts(
      coalesceAdjacentAgentText(
        visibleNormalizedChatItems(mergeMessageRows(olderItems, snapshot.chat.items)),
      ),
    ),
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
