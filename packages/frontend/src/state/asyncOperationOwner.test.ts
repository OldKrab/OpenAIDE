import { describe, expect, it } from "vitest";
import { AsyncOperationOwner } from "./asyncOperationOwner";

describe("AsyncOperationOwner", () => {
  it("gives a result to only the newest operation with the same owner", () => {
    const owner = new AsyncOperationOwner();
    const first = owner.claim("native-sessions");
    const second = owner.claim("native-sessions");

    expect(owner.owns(first)).toBe(false);
    expect(owner.owns(second)).toBe(true);
  });

  it("invalidates every operation when navigation starts", () => {
    const owner = new AsyncOperationOwner();
    const operation = owner.claim("new-task-files");

    owner.beginNavigation("task:task_1");

    expect(owner.owns(operation)).toBe(false);
  });

  it("keeps replica-owned work across navigation but invalidates it on replica replacement", () => {
    const owner = new AsyncOperationOwner();
    const operation = owner.claim("new-task-preparation", "project:agent", "replica");

    owner.beginNavigation("task:task_1");
    expect(owner.owns(operation)).toBe(true);

    owner.replaceReplica();
    expect(owner.owns(operation)).toBe(false);
  });

  it("does not invalidate new work twice when the requested route arrives", () => {
    const owner = new AsyncOperationOwner();
    owner.beginNavigation("task:task_1");
    const operation = owner.claim("task-open");

    owner.observeNavigation("task:task_1");

    expect(owner.owns(operation)).toBe(true);
  });

  it("invalidates work when an external route arrives", () => {
    const owner = new AsyncOperationOwner();
    owner.observeNavigation("task:task_1");
    const operation = owner.claim("task-open");

    owner.observeNavigation("settings:agents");

    expect(owner.owns(operation)).toBe(false);
  });

  it("keeps snapshot ordering private while accepting only the newest result", () => {
    const owner = new AsyncOperationOwner();
    const first = owner.createSnapshotRequest("task_1", "refresh");
    const second = owner.createSnapshotRequest("task_1", "refresh");

    expect(owner.acceptSnapshot("task_1", first, "refresh")).toBe(false);
    expect(owner.acceptSnapshot("task_1", second, "refresh")).toBe(true);
  });

  it("rejects a snapshot that belongs to the previous navigation", () => {
    const owner = new AsyncOperationOwner();
    const requestId = owner.createSnapshotRequest("task_1", "open");

    owner.beginNavigation("settings:general");

    expect(owner.acceptSnapshot("task_1", requestId, "open")).toBe(false);
  });
});
