import { describe, expect, it, vi } from "vitest";
import { AppServerProtocolError, TASK_IMAGE, type BackendConnection } from "@openaide/app-server-client";
import { requestChatImage, type ChatImageLoadLog } from "./chatImageIntents";

function clock(...readings: number[]) {
  // timing: mocked — each read returns the next fixed reading.
  return () => readings.shift() ?? 0;
}

describe("requestChatImage", () => {
  it("requests one Task image and logs its size and duration without the bytes", async () => {
    const request = vi.fn(async () => ({ data: "aGVsbG8=" }));
    const log = vi.fn<ChatImageLoadLog>();

    const loaded = await requestChatImage(
      { request: request as unknown as BackendConnection["request"] },
      "task_1",
      "image_1",
      "original",
      log,
      clock(100, 142),
    );

    expect(loaded).toEqual({ data: "aGVsbG8=" });
    expect(request).toHaveBeenCalledWith(TASK_IMAGE, { taskId: "task_1", imageId: "image_1", variant: "original" });
    expect(log.mock.calls).toEqual([
      ["chat_image_load_started", { task_id: "task_1", image_id: "image_1", variant: "original", cache_hit: false }],
      ["chat_image_load_completed", {
        task_id: "task_1",
        image_id: "image_1",
        variant: "original",
        cache_hit: false,
        duration_ms: 42,
        size_bytes: 5,
        reduced: false,
      }],
    ]);
  });

  it("returns a re-encoded preview with its own media type", async () => {
    const request = vi.fn(async () => ({ data: "aGk=", mediaType: "image/jpeg" }));
    const log = vi.fn<ChatImageLoadLog>();

    const loaded = await requestChatImage(
      { request: request as unknown as BackendConnection["request"] },
      "task_1",
      "image_1",
      "preview",
      log,
      clock(0, 1),
    );

    expect(loaded).toEqual({ data: "aGk=", mediaType: "image/jpeg" });
    expect(request).toHaveBeenCalledWith(TASK_IMAGE, { taskId: "task_1", imageId: "image_1", variant: "preview" });
    expect(log).toHaveBeenLastCalledWith("chat_image_load_completed", expect.objectContaining({
      variant: "preview",
      reduced: true,
      size_bytes: 2,
    }));
  });

  it("classifies a protocol failure without its message and rethrows it", async () => {
    const error = new AppServerProtocolError({
      error: { code: "notFound", message: "image /private/path missing" },
    } as ConstructorParameters<typeof AppServerProtocolError>[0]);
    const request = vi.fn(async () => { throw error; });
    const log = vi.fn<ChatImageLoadLog>();

    await expect(requestChatImage(
      { request: request as unknown as BackendConnection["request"] },
      "task_1",
      "image_1",
      "preview",
      log,
      clock(10, 15),
    )).rejects.toBe(error);

    expect(log).toHaveBeenLastCalledWith("chat_image_load_failed", {
      task_id: "task_1",
      image_id: "image_1",
      variant: "preview",
      cache_hit: false,
      duration_ms: 5,
      error_name: "AppServerProtocolError",
      error_code: "notFound",
    });
    expect(JSON.stringify(log.mock.calls)).not.toContain("private");
  });
});
