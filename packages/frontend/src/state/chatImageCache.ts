/**
 * In-memory copy of the stored Chat images this Frontend has shown (ADR-0075).
 *
 * An image id is the SHA-256 of its bytes, so an id never names different bytes: a cached
 * entry is never stale and is never revalidated, and one entry serves every Task and view
 * that shows the same image. The cache only bounds memory and shares requests; App Server
 * stays the owner of the images and of access to them.
 */
export type ChatImageCache = {
  /** The cached base64 bytes, marking the image as the most recently shown. */
  get(imageId: string): string | undefined;
  /**
   * Returns the image from memory, or from `fetch`. Every view of the same Task image that
   * asks while a request is in flight shares that request. A failure is not cached, so
   * the next call (a Retry) requests again.
   */
  load(taskId: string, imageId: string, fetch: () => Promise<string>): Promise<string>;
  /** Forgets every cached image; in-flight requests still settle for their callers. */
  clear(): void;
  /** Base64 characters held, the quantity the bound applies to. */
  heldBytes(): number;
};

/**
 * Bound on cached base64 text: a few dozen screenshots at typical sizes. JavaScript strings
 * may take up to two bytes per character, so the heap cost can reach twice this figure.
 */
export const CHAT_IMAGE_CACHE_MAX_BYTES = 64 * 1024 * 1024;

export function createChatImageCache({ maxBytes = CHAT_IMAGE_CACHE_MAX_BYTES }: { maxBytes?: number } = {}): ChatImageCache {
  // Map iteration follows insertion order; re-inserting on every read makes the first key
  // the least recently shown, which is the one evicted first.
  const entries = new Map<string, string>();
  // In-flight requests are keyed by Task as well as image: access is checked per Task, so a
  // failure in one Task must not fail the same image shown in another.
  const inFlight = new Map<string, Promise<string>>();
  let held = 0;

  const remember = (imageId: string, data: string) => {
    const previous = entries.get(imageId);
    if (previous !== undefined) {
      entries.delete(imageId);
      held -= previous.length;
    }
    // An image larger than the whole bound is shown but not retained.
    if (data.length > maxBytes) return;
    entries.set(imageId, data);
    held += data.length;
    for (const [oldestId, oldest] of entries) {
      if (held <= maxBytes) break;
      entries.delete(oldestId);
      held -= oldest.length;
    }
  };

  const get = (imageId: string) => {
    const data = entries.get(imageId);
    if (data === undefined) return undefined;
    entries.delete(imageId);
    entries.set(imageId, data);
    return data;
  };

  return {
    get,
    load(taskId, imageId, fetch) {
      const cached = get(imageId);
      if (cached !== undefined) return Promise.resolve(cached);
      const key = `${taskId}\u0000${imageId}`;
      const pending = inFlight.get(key);
      if (pending) return pending;
      const request = Promise.resolve()
        .then(fetch)
        .then(
          (data) => {
            inFlight.delete(key);
            remember(imageId, data);
            return data;
          },
          (error: unknown) => {
            inFlight.delete(key);
            throw error;
          },
        );
      inFlight.set(key, request);
      return request;
    },
    clear() {
      entries.clear();
      held = 0;
    },
    heldBytes: () => held,
  };
}

/** The Frontend's one cache, shared by every App Shell surface that shows a Chat. */
export const chatImageCache = createChatImageCache();
