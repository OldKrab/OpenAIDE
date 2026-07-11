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

/** Keeps the already-rendered window while a live snapshot advances its bounded tail. */
export function retainSnapshotWindow(
  current: ChatPageState | undefined,
  previousPage: MessagePage,
  nextPage: MessagePage,
): ChatPageState | undefined {
  if (previousPage.items.length === 0 && !current) return undefined;
  const nextIds = new Set(nextPage.items.map((item) => item.message_id));
  const retained = mergeMessageRows(current?.olderItems ?? [], previousPage.items)
    .filter((item) => !nextIds.has(item.message_id));
  return {
    olderItems: retained,
    hasBefore: current?.hasBefore ?? previousPage.has_before,
    startCursor: current?.startCursor ?? retained[0]?.cursor ?? previousPage.start_cursor ?? nextPage.start_cursor,
    pending: current?.pending,
    error: current?.error,
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
