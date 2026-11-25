import { describe, expect, it, vi } from "vitest";
import type { LoadedChatImage } from "../intents/chatImageIntents";
import { chatImageCacheKey, createChatImageCache } from "./chatImageCache";

const image = (data: string): LoadedChatImage => ({ data });

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((onResolve, onReject) => {
    resolve = onResolve;
    reject = onReject;
  });
  return { promise, resolve, reject };
}

describe("chatImageCache", () => {
  it("evicts the least recently shown image once the byte bound is exceeded", async () => {
    const cache = createChatImageCache({ maxBytes: 10 });
    await cache.load("task", "a", async () => image("aaaa"));
    await cache.load("task", "b", async () => image("bbbb"));
    // Showing `a` again makes `b` the least recently shown.
    expect(cache.get("a")).toEqual(image("aaaa"));

    await cache.load("task", "c", async () => image("cccc"));

    expect(cache.heldBytes()).toBe(8);
    expect(cache.get("b")).toBeUndefined();
    expect(cache.get("a")).toEqual(image("aaaa"));
    expect(cache.get("c")).toEqual(image("cccc"));
  });

  it("returns but does not retain an image larger than the whole bound", async () => {
    const cache = createChatImageCache({ maxBytes: 4 });
    await cache.load("task", "small", async () => image("ss"));

    await expect(cache.load("task", "large", async () => image("llllll"))).resolves.toEqual(image("llllll"));

    expect(cache.get("large")).toBeUndefined();
    expect(cache.get("small")).toEqual(image("ss"));
  });

  it("serves a cached image without requesting it again", async () => {
    const cache = createChatImageCache();
    const fetch = vi.fn(async () => image("data"));
    await cache.load("task", "image", fetch);

    await expect(cache.load("other-task", "image", fetch)).resolves.toEqual(image("data"));

    expect(fetch).toHaveBeenCalledOnce();
  });

  it("shares one in-flight request among every view of the same image", async () => {
    const cache = createChatImageCache();
    const response = deferred<LoadedChatImage>();
    const fetch = vi.fn(() => response.promise);

    const first = cache.load("task", "image", fetch);
    const second = cache.load("task", "image", fetch);
    response.resolve(image("data"));

    await expect(Promise.all([first, second])).resolves.toEqual([image("data"), image("data")]);
    expect(fetch).toHaveBeenCalledOnce();
  });

  it("does not cache a failure, so a retry requests again", async () => {
    const cache = createChatImageCache();
    const failed = deferred<LoadedChatImage>();
    const fetch = vi.fn()
      .mockImplementationOnce(() => failed.promise)
      .mockImplementationOnce(async () => image("data"));

    const first = cache.load("task", "image", fetch);
    const joined = cache.load("task", "image", fetch);
    failed.reject(new Error("missing"));
    await expect(first).rejects.toThrow("missing");
    await expect(joined).rejects.toThrow("missing");
    expect(cache.get("image")).toBeUndefined();

    await expect(cache.load("task", "image", fetch)).resolves.toEqual(image("data"));
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("keeps a failure in one Task from failing the same image requested by another", async () => {
    const cache = createChatImageCache();
    const failed = deferred<LoadedChatImage>();

    const denied = cache.load("task-a", "image", () => failed.promise);
    const allowed = cache.load("task-b", "image", async () => image("data"));
    failed.reject(new Error("notFound"));

    await expect(denied).rejects.toThrow("notFound");
    await expect(allowed).resolves.toEqual(image("data"));
  });

  it("keeps the preview and the original of one image apart", async () => {
    const cache = createChatImageCache();
    const preview = { data: "small", mediaType: "image/jpeg" };
    await cache.load("task", chatImageCacheKey("image", "preview"), async () => preview);

    const fetchOriginal = vi.fn(async () => image("large"));
    await expect(cache.load("task", chatImageCacheKey("image", "original"), fetchOriginal))
      .resolves.toEqual(image("large"));

    expect(fetchOriginal).toHaveBeenCalledOnce();
    expect(cache.get(chatImageCacheKey("image", "preview"))).toEqual(preview);
  });

  it("forgets every image when cleared", async () => {
    const cache = createChatImageCache();
    await cache.load("task", "image", async () => image("data"));

    cache.clear();

    expect(cache.get("image")).toBeUndefined();
    expect(cache.heldBytes()).toBe(0);
  });
});
