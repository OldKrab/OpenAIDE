import { describe, expect, it, vi } from "vitest";
import {
  AppServerProtocolError,
  TASK_ACQUIRE_IN_WORKTREE,
  TASK_RELEASE,
  type BackendConnection,
} from "@openaide/app-server-client";
import { acquirePreparedTaskWithConflictRetry } from "./newTaskLeaseRecovery";

describe("New Task lease recovery", () => {
  it("releases the exact conflicting Prepared Task, awaits acknowledgement, and retries once", async () => {
    let released = false;
    const requestMock = vi.fn(async (method: string, params: unknown) => {
      if (method === TASK_RELEASE) {
        expect(params).toEqual({ taskId: "task_stale" });
        released = true;
        return { taskId: "task_stale" };
      }
      if (method === TASK_ACQUIRE_IN_WORKTREE) {
        if (!released) throw conflict("task_stale");
        return { task: { task: { taskId: "task_current" } } };
      }
      throw new Error(`Unexpected request: ${method}`);
    });
    const request = requestMock as unknown as BackendConnection["request"];

    await expect(acquirePreparedTaskWithConflictRetry(
      request,
      () => request(TASK_ACQUIRE_IN_WORKTREE, {} as never),
    )).resolves.toMatchObject({ task: { task: { taskId: "task_current" } } });
    expect(requestMock.mock.calls.map(([method]) => method)).toEqual([
      TASK_ACQUIRE_IN_WORKTREE,
      TASK_RELEASE,
      TASK_ACQUIRE_IN_WORKTREE,
    ]);
  });

  it("never releases without an exact authoritative Prepared Task target", async () => {
    const requestMock = vi.fn(async () => ({ taskId: "task_stale" }));
    const request = requestMock as unknown as BackendConnection["request"];
    const error = conflict();

    await expect(acquirePreparedTaskWithConflictRetry(request, async () => {
      throw error;
    })).rejects.toBe(error);
    expect(requestMock).not.toHaveBeenCalled();
  });

  it("does not steal a lease by retrying or releasing a second conflict", async () => {
    const requestMock = vi.fn(async () => ({ taskId: "task_owned" }));
    const request = requestMock as unknown as BackendConnection["request"];
    const acquire = vi.fn()
      .mockRejectedValueOnce(conflict("task_owned"))
      .mockRejectedValueOnce(conflict("task_other"));

    await expect(acquirePreparedTaskWithConflictRetry(request, acquire)).rejects.toMatchObject({
      protocolError: { target: { currentTask: { task: { taskId: "task_other" } } } },
    });
    expect(requestMock).toHaveBeenCalledOnce();
    expect(requestMock).toHaveBeenCalledWith(TASK_RELEASE, { taskId: "task_owned" });
    expect(acquire).toHaveBeenCalledTimes(2);
  });

  it("does not retry when release acknowledges a different task", async () => {
    const requestMock = vi.fn(async () => ({ taskId: "task_other" }));
    const request = requestMock as unknown as BackendConnection["request"];
    const acquire = vi.fn().mockRejectedValue(conflict("task_owned"));

    await expect(acquirePreparedTaskWithConflictRetry(request, acquire)).rejects.toThrow(
      "Release acknowledged task_other instead of task_owned",
    );
    expect(requestMock).toHaveBeenCalledOnce();
    expect(acquire).toHaveBeenCalledOnce();
  });

  it("propagates release failure without retrying acquisition", async () => {
    const releaseError = new Error("release failed");
    const requestMock = vi.fn(async () => {
      throw releaseError;
    });
    const request = requestMock as unknown as BackendConnection["request"];
    const acquire = vi.fn().mockRejectedValue(conflict("task_owned"));

    await expect(acquirePreparedTaskWithConflictRetry(request, acquire)).rejects.toBe(releaseError);
    expect(requestMock).toHaveBeenCalledOnce();
    expect(acquire).toHaveBeenCalledOnce();
  });
});

function conflict(taskId?: string) {
  return new AppServerProtocolError({
    error: {
      code: "conflict",
      message: "Release the current Prepared Task before acquiring another context",
      recoverable: true,
      ...(taskId ? {
        target: {
          currentTask: {
            task: { taskId },
            lifecycle: "new",
          } as never,
        },
      } : {}),
    },
  });
}
