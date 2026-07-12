import { describe, expect, it, vi } from "vitest";
import { TASK_DISCARD, type BackendConnection } from "@openaide/app-server-client";
import { PreparedTaskOwnership } from "./preparedTaskOwnership";

describe("prepared Task ownership", () => {
  it("keeps an ambiguous send protected after a newer prepared Task is claimed", async () => {
    const request = vi.fn(async () => ({ discarded: true })) as unknown as BackendConnection["request"];
    const dispatch = vi.fn();
    const ownership = new PreparedTaskOwnership();
    const leaseA = ownership.claim({ preparationKey: "context-a", taskId: "task_a" as never });
    ownership.protectSend(leaseA, "send-a");
    const leaseB = ownership.claim({ preparationKey: "context-b", taskId: "task_b" as never });

    await ownership.discard({ dispatch, lease: leaseA, request, taskId: "task_a" as never });
    await ownership.discard({ dispatch, lease: leaseB, request, taskId: "task_b" as never });

    expect(request).not.toHaveBeenCalledWith(TASK_DISCARD, { taskId: "task_a" });
    expect(request).toHaveBeenCalledWith(TASK_DISCARD, { taskId: "task_b" });
  });

  it("does not let an older rejected send reclaim a newer prepared Task lease", () => {
    const ownership = new PreparedTaskOwnership();
    const leaseA = ownership.claim({ preparationKey: "context-a", taskId: "task_a" as never });
    ownership.protectSend(leaseA, "send-a");
    const leaseB = ownership.claim({ preparationKey: "context-b", taskId: "task_b" as never });

    ownership.settleSend("send-a");

    expect(ownership.reclaim(leaseA)).toBe(false);
    expect(ownership.currentLease()).toBe(leaseB);
  });

  it("forgets old-root disposal and lease identities before a colliding Task id is reused", async () => {
    const request = vi.fn(async () => ({ discarded: true })) as unknown as BackendConnection["request"];
    const dispatch = vi.fn();
    const ownership = new PreparedTaskOwnership();
    const oldLease = ownership.claim({ preparationKey: "root-a", taskId: "task_1" as never });
    ownership.recordDiscarded("task_1" as never);

    ownership.replaceStateRoot();

    expect(ownership.isCurrent(oldLease)).toBe(false);
    expect(ownership.isDisposable("task_1")).toBe(true);
    const newLease = ownership.claim({ preparationKey: "root-b", taskId: "task_1" as never });
    await ownership.discard({ dispatch, lease: newLease, request, taskId: "task_1" as never });
    expect(request).toHaveBeenCalledWith(TASK_DISCARD, { taskId: "task_1" });
  });
});
