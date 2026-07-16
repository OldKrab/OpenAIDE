import { describe, expect, it, vi } from "vitest";
import {
  AppServerProtocolError,
  TASK_CANCEL,
  TASK_RELEASE,
  type BackendConnection,
} from "@openaide/app-server-client";
import { discardOrCancelStartedTask } from "./newTaskStartSupport";

describe("new Task startup cleanup", () => {
  it("cancels only when an authoritative discard conflict proves Send won the race", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === TASK_RELEASE) {
        throw new AppServerProtocolError({
          error: { code: "conflict", message: "Task already has messages", recoverable: true },
        });
      }
      if (method === TASK_CANCEL) return { cancelled: true };
      throw new Error(method);
    }) as unknown as BackendConnection["request"];

    await expect(discardOrCancelStartedTask(request, "task_1" as never)).resolves.toBe("cancelled");
    expect(request).toHaveBeenCalledWith(TASK_CANCEL, { taskId: "task_1" });
  });

  it("does not turn an ambiguous discard transport failure into a no-op cancel", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === TASK_RELEASE) throw new Error("connection closed");
      if (method === TASK_CANCEL) return { cancelled: true };
      throw new Error(method);
    }) as unknown as BackendConnection["request"];

    await expect(discardOrCancelStartedTask(request, "task_1" as never)).rejects.toThrow("connection closed");
    expect(request).not.toHaveBeenCalledWith(TASK_CANCEL, { taskId: "task_1" });
  });
});
