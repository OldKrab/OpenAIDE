import { afterEach, describe, expect, it, vi } from "vitest";
import {
  AppServerProtocolError,
  type BackendConnection,
} from "@openaide/app-server-client";
import { readPendingTaskSendRecovery } from "./pendingTaskSendRecovery";
import {
  executeTaskSendAttempt,
  inFlightTaskSendAttempt,
  resolveTaskSendAttempt,
  taskSendAttemptRecord,
} from "./taskSendAttempt";

describe("task send attempt", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("keeps the exact attempt recoverable until task/send responds", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
    const response = deferred<never>();
    const request = vi.fn(() => response.promise) as unknown as BackendConnection["request"];
    const attempt = recoveryAttempt();

    const pending = executeTaskSendAttempt({ attempt, backendConnection: { request } });

    expect(readPendingTaskSendRecovery("root-a", "client-a", "task-a")).toEqual(attempt);
    response.resolve({} as never);
    await pending;
    expect(readPendingTaskSendRecovery("root-a", "client-a", "task-a")).toBeUndefined();
  });

  it("joins duplicate in-process recovery onto the same task/send request", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
    const response = deferred<never>();
    const request = vi.fn(() => response.promise) as unknown as BackendConnection["request"];
    const attempt = recoveryAttempt();

    const original = executeTaskSendAttempt({ attempt, backendConnection: { request } });
    const recovery = executeTaskSendAttempt({ attempt, backendConnection: { request } });

    expect(request).toHaveBeenCalledTimes(1);
    response.resolve({} as never);
    await expect(Promise.all([original, recovery])).resolves.toHaveLength(2);
  });

  it("does not join colliding Task sends from different state roots", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
    const firstResponse = deferred<never>();
    const secondResponse = deferred<never>();
    const request = vi.fn()
      .mockReturnValueOnce(firstResponse.promise)
      .mockReturnValueOnce(secondResponse.promise) as unknown as BackendConnection["request"];

    const first = executeTaskSendAttempt({
      attempt: recoveryAttempt("root-1"),
      backendConnection: { request },
    });
    const second = executeTaskSendAttempt({
      attempt: recoveryAttempt("root-2"),
      backendConnection: { request },
    });

    expect(request).toHaveBeenCalledTimes(2);
    firstResponse.resolve({} as never);
    secondResponse.resolve({} as never);
    await Promise.all([first, second]);
  });

  it("exposes only the exact in-flight send for Stop sequencing", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
    const response = deferred<never>();
    const request = vi.fn(() => response.promise) as unknown as BackendConnection["request"];
    const attempt = recoveryAttempt();

    const pending = executeTaskSendAttempt({ attempt, backendConnection: { request } });

    expect(inFlightTaskSendAttempt({
      clientInstanceId: "client-a",
      stateRootId: "root-a",
      taskId: "task-a",
      idempotencyKey: "send-a" as never,
    })).toBe(pending);
    expect(inFlightTaskSendAttempt({
      clientInstanceId: "client-a",
      stateRootId: "root-a",
      taskId: "task-a",
      idempotencyKey: "different-send" as never,
    })).toBeUndefined();

    response.resolve({} as never);
    await pending;
    expect(inFlightTaskSendAttempt({
      clientInstanceId: "client-a",
      stateRootId: "root-a",
      taskId: "task-a",
      idempotencyKey: "send-a" as never,
    })).toBeUndefined();
  });

  it("retains ambiguous transport failures but clears authoritative rejections", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
    const attempt = recoveryAttempt();
    const transportRequest = vi.fn(async () => {
      throw new Error("connection closed");
    }) as unknown as BackendConnection["request"];

    await expect(executeTaskSendAttempt({
      attempt,
      backendConnection: { request: transportRequest },
    })).rejects.toThrow("connection closed");
    expect(readPendingTaskSendRecovery("root-a", "client-a", "task-a")?.idempotencyKey).toBe("send-a");

    const protocolRequest = vi.fn(async () => {
      throw new AppServerProtocolError({
        error: { code: "conflict", message: "Task rejected the send", recoverable: true },
      });
    }) as unknown as BackendConnection["request"];
    await expect(executeTaskSendAttempt({
      attempt,
      backendConnection: { request: protocolRequest },
    })).rejects.toThrow("Task rejected the send");
    expect(readPendingTaskSendRecovery("root-a", "client-a", "task-a")).toBeUndefined();
  });

  it("retries a stale revision from the error render state without reopening the Task", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
    const attempt = recoveryAttempt();
    const request = vi.fn()
      .mockRejectedValueOnce(new AppServerProtocolError({
        error: {
          code: "conflict",
          message: "Task changed before the message was sent",
          recoverable: true,
          target: {
            field: "taskRevision",
            currentTask: { task: { taskId: "task-a" }, revision: 9 } as never,
          },
        },
      }))
      .mockResolvedValueOnce({ task: { revision: 10 } });

    const execution = await executeTaskSendAttempt({
      attempt,
      backendConnection: { request: request as never },
      refreshRevisionOnConflict: true,
    });

    expect(request).toHaveBeenCalledTimes(2);
    expect(request.mock.calls.map(([method]) => method)).toEqual(["task/send", "task/send"]);
    expect(request.mock.calls[0]?.[1]).toMatchObject({ taskRevision: 3 });
    expect(request.mock.calls[1]?.[1]).toMatchObject({ taskRevision: 9 });
    expect(execution.attempt.taskRevision).toBe(9);
  });

  it("does not retry a non-revision conflict", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
    const request = vi.fn().mockRejectedValue(new AppServerProtocolError({
      error: { code: "conflict", message: "Task is already running", recoverable: true },
    }));

    await expect(executeTaskSendAttempt({
      attempt: recoveryAttempt(),
      backendConnection: { request: request as never },
      refreshRevisionOnConflict: true,
    })).rejects.toThrow("Task is already running");

    expect(request).toHaveBeenCalledTimes(1);
  });

  it("restores the exact ambiguous attempt even when the current composer draft changed", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
    const original = recoveryAttempt();
    const request = vi.fn(async () => {
      throw new Error("connection closed");
    }) as unknown as BackendConnection["request"];

    await expect(executeTaskSendAttempt({
      attempt: original,
      backendConnection: { request },
    })).rejects.toThrow("connection closed");

    const resolved = resolveTaskSendAttempt(taskSendAttemptRecord({
      ...original,
      idempotencyKey: "send-new" as never,
      message: { text: "Edited after failure" },
      renderState: { prompt: "Edited after failure", context: [] },
    }));

    expect(resolved).toEqual(original);
  });
});

function recoveryAttempt(stateRootId = "root-a") {
  return taskSendAttemptRecord({
    clientInstanceId: "client-a",
    idempotencyKey: "send-a" as never,
    message: { text: "Ship it" },
    renderState: { prompt: "Ship it", context: [] },
    stateRootId,
    taskId: "task-a",
    taskRevision: 3,
  });
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => {
      values.delete(key);
    },
    setItem: (key, value) => {
      values.set(key, value);
    },
  };
}
