import { describe, expect, it, vi } from "vitest";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import { TASK_RELEASE, type BackendConnection } from "@openaide/app-server-client";
import { createInitialState } from "../state/store";
import { disposableNewTaskControllerId, NewTaskController } from "./newTaskController";

describe("New Task controller", () => {
  it("keeps one lease when the same Prepared Task is claimed through a newer render key", () => {
    const controller = new NewTaskController();
    const originalLease = controller.claim({
      preparationKey: "render-a",
      taskId: "task_1" as never,
    });

    const reclaimedLease = controller.claim({
      preparationKey: "render-b",
      taskId: "task_1" as never,
    });

    expect(reclaimedLease).toBe(originalLease);
    expect(controller.isCurrent(originalLease)).toBe(true);
  });

  it("keeps an in-flight send protected after a newer New Task is claimed", async () => {
    const request = vi.fn(async () => ({ discarded: true })) as unknown as BackendConnection["request"];
    const dispatch = vi.fn();
    const controller = new NewTaskController();
    const leaseA = controller.claim({ preparationKey: "context-a", taskId: "task_a" as never });
    controller.protectSend(leaseA);
    const leaseB = controller.claim({ preparationKey: "context-b", taskId: "task_b" as never });

    await controller.discard({ dispatch, lease: leaseA, request, taskId: "task_a" as never });
    await controller.discard({ dispatch, lease: leaseB, request, taskId: "task_b" as never });

    expect(request).not.toHaveBeenCalledWith(TASK_RELEASE, { taskId: "task_a" });
    expect(request).toHaveBeenCalledWith(TASK_RELEASE, { taskId: "task_b" });
  });

  it("does not let an older rejected send reclaim a newer New Task lease", () => {
    const controller = new NewTaskController();
    const leaseA = controller.claim({ preparationKey: "context-a", taskId: "task_a" as never });
    controller.protectSend(leaseA);
    const leaseB = controller.claim({ preparationKey: "context-b", taskId: "task_b" as never });

    controller.settleSend("task_a");

    expect(controller.reclaim(leaseA)).toBe(false);
    expect(controller.currentLease()).toBe(leaseB);
  });

  it("forgets old-root disposal and lease identities before a colliding Task id is reused", async () => {
    const request = vi.fn(async () => ({ discarded: true })) as unknown as BackendConnection["request"];
    const dispatch = vi.fn();
    const controller = new NewTaskController();
    const oldLease = controller.claim({ preparationKey: "root-a", taskId: "task_1" as never });
    controller.recordDiscarded("task_1" as never);

    controller.replaceStateRoot();

    expect(controller.isCurrent(oldLease)).toBe(false);
    expect(controller.isDisposable("task_1")).toBe(true);
    const newLease = controller.claim({ preparationKey: "root-b", taskId: "task_1" as never });
    await controller.discard({ dispatch, lease: newLease, request, taskId: "task_1" as never });
    expect(request).toHaveBeenCalledWith(TASK_RELEASE, { taskId: "task_1" });
  });

  it("publishes cache removal when a state-root replacement clears the New Task", () => {
    const controller = new NewTaskController();
    const listener = vi.fn();
    controller.subscribe(listener);
    const snapshot = {
      lifecycle: "prepared",
      task: { task_id: "task_1" },
    } as TaskSnapshot;

    controller.retain({ preparationKey: "root-a", snapshot });
    expect(controller.getSnapshot()).toBe(snapshot);
    listener.mockClear();

    controller.replaceStateRoot();

    expect(controller.getSnapshot()).toBeUndefined();
    expect(listener).toHaveBeenCalledOnce();
  });

  it("ignores the expired lease's stale baseline until the Task is acquired again", () => {
    const controller = new NewTaskController();
    const snapshot = {
      lifecycle: "prepared",
      task: { task_id: "task_1" },
    } as TaskSnapshot;
    controller.retain({ preparationKey: "context-a", snapshot });

    expect(controller.expireClientLease()).toBe("task_1");
    expect(controller.updateSnapshot(snapshot)).toBe(false);
    expect(controller.getSnapshot()).toBeUndefined();

    expect(controller.retain({ preparationKey: "context-a", snapshot })).toBeDefined();
    expect(controller.getSnapshot()).toBe(snapshot);
  });

  it("keeps settled Agent controls while an expired Task replacement is preparing", () => {
    const controller = new NewTaskController();
    const settledCatalog = {
      agent_id: "codex",
      options: [],
      status: "empty",
    } as const;
    controller.retain({
      preparationKey: "context-a",
      snapshot: {
        lifecycle: "prepared",
        task: { agent_id: "codex", task_id: "task_1" },
        agent_config: settledCatalog,
      } as unknown as TaskSnapshot,
    });
    controller.expireClientLease();

    controller.retain({
      preparationKey: "context-a",
      snapshot: {
        lifecycle: "prepared",
        task: { agent_id: "codex", task_id: "task_2" },
        agent_config: { agent_id: "codex", options: [], status: "loading" },
      } as unknown as TaskSnapshot,
    });

    expect(controller.getSnapshot()?.agent_config).toBe(settledCatalog);
  });

  it("does not replace settled controls with a transient loading update for the same Task", () => {
    const controller = new NewTaskController();
    const settledCatalog = {
      agent_id: "codex",
      options: [],
      status: "empty",
    } as const;
    controller.retain({
      preparationKey: "context-a",
      snapshot: {
        lifecycle: "prepared",
        task: { agent_id: "codex", task_id: "task_1" },
        agent_config: settledCatalog,
      } as unknown as TaskSnapshot,
    });

    controller.updateSnapshot({
      lifecycle: "prepared",
      task: { agent_id: "codex", task_id: "task_1" },
      agent_config: { agent_id: "codex", options: [], status: "loading" },
    } as unknown as TaskSnapshot);

    expect(controller.getSnapshot()?.agent_config).toBe(settledCatalog);
  });

  it("does not replace a ready Task with an older preparation snapshot", () => {
    const controller = new NewTaskController();
    const ready = {
      lifecycle: "prepared",
      revision: 2,
      task: { agent_id: "codex", task_id: "task_1" },
      preparation: { kind: "ready" },
      send_capability: { state: "ready" },
    } as unknown as TaskSnapshot;
    controller.retain({ preparationKey: "context-a", snapshot: ready });

    expect(controller.updateSnapshot({
      lifecycle: "prepared",
      revision: 1,
      task: { agent_id: "codex", task_id: "task_1" },
      preparation: { kind: "preparing" },
      send_capability: {
        state: "loading",
        blockers: [{ kind: "taskPreparing", message: "Task Agent preparation is still running" }],
      },
    } as unknown as TaskSnapshot)).toBe(true);

    expect(controller.getSnapshot()).toBe(ready);
  });

  it("never treats a visible zero-message Task as disposable New Task state", () => {
    const controller = new NewTaskController();
    const state = createInitialState();
    state.snapshot = {
      lifecycle: "open",
      task: { has_messages: false, task_id: "task_1" },
    } as TaskSnapshot;
    controller.claim({ preparationKey: "context-a", taskId: "task_1" as never });

    expect(disposableNewTaskControllerId(state, controller)).toBeUndefined();
  });
});
