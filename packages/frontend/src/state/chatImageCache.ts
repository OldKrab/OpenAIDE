import type { TaskImageVariant } from "@openaide/app-server-client";
import type { LoadedChatImage } from "../intents/chatImageIntents";

/**
 * In-memory copy of the stored Chat images this Frontend has shown (ADR-0075), keyed by
 * image and variant (ADR-0084): a Chat row holds the preview, the full-size view the
 * original.
 *
 * An image id is the SHA-256 of its bytes, so an id never names different bytes: a cached
 * entry is never stale and is never revalidated, and one entry serves every Task and view
 * that shows the same image. The cache only bounds memory and shares requests; App Server
 * stays the owner of the images and of access to them.
 */
export type ChatImageCache = {
  /** The cached image, marking it as the most recently shown. */
  get(key: string): LoadedChatImage | undefined;
  /**
   * Returns the image from memory, or from `fetch`. Every view of the same Task image that
   * asks while a request is in flight shares that request. A failure is not cached, so
   * the next call (a Retry) requests again.
   */
  load(taskId: string, key: string, fetch: () => Promise<LoadedChatImage>): Promise<LoadedChatImage>;
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

/** The cache key of one copy of one stored image. */
export function chatImageCacheKey(imageId: string, variant: TaskImageVariant) {
  return `${imageId}:${variant}`;
}

export function createChatImageCache({ maxBytes = CHAT_IMAGE_CACHE_MAX_BYTES }: { maxBytes?: number } = {}): ChatImageCache {
  // Map iteration follows insertion order; re-inserting on every read makes the first key
  // the least recently shown, which is the one evicted first.
  const entries = new Map<string, LoadedChatImage>();
  // In-flight requests are keyed by Task as well as image: access is checked per Task, so a
  // failure in one Task must not fail the same image shown in another.
  const inFlight = new Map<string, Promise<LoadedChatImage>>();
  let held = 0;

  const remember = (key: string, image: LoadedChatImage) => {
    const previous = entries.get(key);
    if (previous !== undefined) {
      entries.delete(key);
      held -= previous.data.length;
    }
    // An image larger than the whole bound is shown but not retained.
    if (image.data.length > maxBytes) return;
    entries.set(key, image);
    held += image.data.length;
    for (const [oldestKey, oldest] of entries) {
      if (held <= maxBytes) break;
      entries.delete(oldestKey);
      held -= oldest.data.length;
    }
  };

  const get = (key: string) => {
    const image = entries.get(key);
    if (image === undefined) return undefined;
    entries.delete(key);
    entries.set(key, image);
    return image;
  };

  return {
    get,
    load(taskId, key, fetch) {
      const cached = get(key);
      if (cached !== undefined) return Promise.resolve(cached);
      const requestKey = `${taskId}\u0000${key}`;
      const pending = inFlight.get(requestKey);
      if (pending) return pending;
      const request = Promise.resolve()
        .then(fetch)
        .then(
          (image) => {
            inFlight.delete(requestKey);
            remember(key, image);
            return image;
          },
          (error: unknown) => {
            inFlight.delete(requestKey);
            throw error;
          },
        );
      inFlight.set(requestKey, request);
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
