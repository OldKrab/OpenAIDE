import { describe, expect, it, vi } from "vitest";
import { createChatImageCache } from "./chatImageCache";

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
    await cache.load("task", "a", async () => "aaaa");
    await cache.load("task", "b", async () => "bbbb");
    // Showing `a` again makes `b` the least recently shown.
    expect(cache.get("a")).toBe("aaaa");

    await cache.load("task", "c", async () => "cccc");

    expect(cache.heldBytes()).toBe(8);
    expect(cache.get("b")).toBeUndefined();
    expect(cache.get("a")).toBe("aaaa");
    expect(cache.get("c")).toBe("cccc");
  });

  it("returns but does not retain an image larger than the whole bound", async () => {
    const cache = createChatImageCache({ maxBytes: 4 });
    await cache.load("task", "small", async () => "ss");

    await expect(cache.load("task", "large", async () => "llllll")).resolves.toBe("llllll");

    expect(cache.get("large")).toBeUndefined();
    expect(cache.get("small")).toBe("ss");
  });

  it("serves a cached image without requesting it again", async () => {
    const cache = createChatImageCache();
    const fetch = vi.fn(async () => "data");
    await cache.load("task", "image", fetch);

    await expect(cache.load("other-task", "image", fetch)).resolves.toBe("data");

    expect(fetch).toHaveBeenCalledOnce();
  });

  it("shares one in-flight request among every view of the same image", async () => {
    const cache = createChatImageCache();
    const response = deferred<string>();
    const fetch = vi.fn(() => response.promise);

    const first = cache.load("task", "image", fetch);
    const second = cache.load("task", "image", fetch);
    response.resolve("data");

    await expect(Promise.all([first, second])).resolves.toEqual(["data", "data"]);
    expect(fetch).toHaveBeenCalledOnce();
  });

  it("does not cache a failure, so a retry requests again", async () => {
    const cache = createChatImageCache();
    const failed = deferred<string>();
    const fetch = vi.fn()
      .mockImplementationOnce(() => failed.promise)
      .mockImplementationOnce(async () => "data");

    const first = cache.load("task", "image", fetch);
    const joined = cache.load("task", "image", fetch);
    failed.reject(new Error("missing"));
    await expect(first).rejects.toThrow("missing");
    await expect(joined).rejects.toThrow("missing");
    expect(cache.get("image")).toBeUndefined();

    await expect(cache.load("task", "image", fetch)).resolves.toBe("data");
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("keeps a failure in one Task from failing the same image requested by another", async () => {
    const cache = createChatImageCache();
    const failed = deferred<string>();

    const denied = cache.load("task-a", "image", () => failed.promise);
    const allowed = cache.load("task-b", "image", async () => "data");
    failed.reject(new Error("notFound"));

    await expect(denied).rejects.toThrow("notFound");
    await expect(allowed).resolves.toBe("data");
  });

  it("forgets every image when cleared", async () => {
    const cache = createChatImageCache();
    await cache.load("task", "image", async () => "data");

    cache.clear();

    expect(cache.get("image")).toBeUndefined();
    expect(cache.heldBytes()).toBe(0);
  });
});
