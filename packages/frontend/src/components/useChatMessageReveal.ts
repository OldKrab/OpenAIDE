import { useEffect, useRef, useSyncExternalStore } from "react";
import { chatReveal, type ChatRevealStore } from "../state/chatReveal";
import type { TaskChatTimelineRow } from "./TaskChatTimeline";

/** Earlier-page loads one reveal may trigger before it gives up on a message. */
const MAX_REVEAL_PAGES = 40;

type RevealOptions = {
  beforeCursor?: string;
  hasEarlier: boolean;
  loadEarlier: (beforeCursor: string) => number | undefined;
  /** Stops Follow from pulling the view back to the latest message. */
  pauseFollowing: () => void;
  pending: boolean;
  rows: readonly TaskChatTimelineRow[];
  scrollToIndex: (index: number) => void;
  store?: ChatRevealStore;
  taskId: string;
};

/**
 * Scrolls the Chat to a message a search hit asked for. It loads earlier pages until the
 * message is in the rendered rows, then centers it. Declare it after `useTaskChatScroll`:
 * that hook restores the saved position in layout effects, and this one must scroll after.
 */
export function useChatMessageReveal({
  beforeCursor,
  hasEarlier,
  loadEarlier,
  pauseFollowing,
  pending,
  rows,
  scrollToIndex,
  store = chatReveal,
  taskId,
}: RevealOptions) {
  const snapshot = useSyncExternalStore(store.subscribe, store.getSnapshot);
  const pagesLoaded = useRef(0);
  const request = snapshot.request;
  useEffect(() => {
    const live = store.pendingFor(taskId);
    if (!live) {
      pagesLoaded.current = 0;
      return;
    }
    const index = rows.findIndex((row) => row.kind === "message" && row.message.message_id === live.messageId);
    if (index >= 0) {
      pauseFollowing();
      scrollToIndex(index);
      pagesLoaded.current = 0;
      store.settle(live, true);
      return;
    }
    if (pending) return;
    const hasMessages = rows.some((row) => row.kind === "message");
    // The Task's Chat has not arrived yet; the rows change when it does.
    if (!hasMessages && !hasEarlier) return;
    if (hasEarlier && beforeCursor && pagesLoaded.current < MAX_REVEAL_PAGES) {
      pagesLoaded.current += 1;
      loadEarlier(beforeCursor);
      return;
    }
    console.info("chat_message_reveal_not_found", { task_id: taskId, pages_loaded: pagesLoaded.current });
    pagesLoaded.current = 0;
    store.settle(live, false);
  }, [beforeCursor, hasEarlier, loadEarlier, pauseFollowing, pending, request, rows, scrollToIndex, store, taskId]);
  return snapshot.highlighted?.taskId === taskId ? snapshot.highlighted.messageId : undefined;
}
