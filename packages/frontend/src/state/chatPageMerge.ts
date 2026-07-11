import type { ChatMessage, MessagePage } from "@openaide/app-shell-contracts";
import type { ChatPageState } from "./store";

export function mergePageState(current: ChatPageState | undefined, page: MessagePage): ChatPageState {
  const existing = current?.olderItems ?? [];
  const olderItems = mergeMessageRows(page.items, existing);
  return {
    olderItems,
    hasBefore: page.has_before,
    startCursor: page.start_cursor ?? olderItems[0]?.cursor,
    pending: false,
  };
}

export function mergeMessageRows(left: ChatMessage[], right: ChatMessage[]) {
  const seen = new Set<string>();
  const merged: ChatMessage[] = [];
  for (const item of [...left, ...right]) {
    if (seen.has(item.message_id)) continue;
    seen.add(item.message_id);
    merged.push(item);
  }
  return merged;
}
