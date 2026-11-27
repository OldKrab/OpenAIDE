/**
 * Ephemeral request to scroll an opened Task's Chat to one message, made by a search hit.
 * It is presentation state only: App Server owns the Chat, this carries where the Frontend
 * should look. A request expires so one that never matches cannot fire on a later visit.
 */
export type ChatRevealRequest = { taskId: string; messageId: string; expiresAt: number };

export type ChatRevealSnapshot = {
  request?: ChatRevealRequest;
  /** The message to mark briefly after a reveal, so the eye finds it. */
  highlighted?: { taskId: string; messageId: string };
};

/** A request that has not matched this long after the click is dropped. */
export const REVEAL_REQUEST_TTL_MS = 15_000;
/** How long the revealed message stays marked. */
export const REVEAL_HIGHLIGHT_MS = 2_400;

export type ChatRevealStore = ReturnType<typeof createChatRevealStore>;

export function createChatRevealStore(
  now: () => number = Date.now,
  schedule: (callback: () => void, delayMs: number) => unknown = (callback, delayMs) => setTimeout(callback, delayMs),
) {
  let snapshot: ChatRevealSnapshot = {};
  const listeners = new Set<() => void>();
  const set = (next: ChatRevealSnapshot) => {
    snapshot = next;
    for (const listener of [...listeners]) listener();
  };
  return {
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
    getSnapshot: () => snapshot,
    /** Asks the Task's Chat to reveal a message once it is on screen. */
    request(taskId: string, messageId: string) {
      set({ ...snapshot, request: { taskId, messageId, expiresAt: now() + REVEAL_REQUEST_TTL_MS } });
    },
    /** The live request for this Task, or none when it is for another Task or expired. */
    pendingFor(taskId: string): ChatRevealRequest | undefined {
      const request = snapshot.request;
      if (!request || request.taskId !== taskId) return undefined;
      if (request.expiresAt <= now()) {
        set({ ...snapshot, request: undefined });
        return undefined;
      }
      return request;
    },
    /** Ends the request: found, or the message is not in this Chat. */
    settle(request: ChatRevealRequest, found: boolean) {
      if (snapshot.request !== request) return;
      if (!found) {
        set({ ...snapshot, request: undefined });
        return;
      }
      const highlighted = { taskId: request.taskId, messageId: request.messageId };
      set({ request: undefined, highlighted });
      schedule(() => {
        if (snapshot.highlighted === highlighted) set({ ...snapshot, highlighted: undefined });
      }, REVEAL_HIGHLIGHT_MS);
    },
  };
}

/** The app's one reveal channel between the sidebar and the Chat view. */
export const chatReveal = createChatRevealStore();
