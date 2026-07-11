import { describe, expect, it } from "vitest";
import { SnapshotRequestTracker } from "./snapshotRequests";

describe("SnapshotRequestTracker", () => {
  it("accepts the newest request for a task", () => {
    const tracker = new SnapshotRequestTracker();
    const first = tracker.create("task_1");
    const second = tracker.create("task_1");

    expect(tracker.accept("task_1", first, "refresh")).toMatchObject({
      accepted: false,
      reason: "older_task_request",
      latestSnapshotRequestId: second,
    });
    expect(tracker.accept("task_1", second, "refresh")).toEqual({ accepted: true });
  });

  it("rejects snapshots from prior navigation generations", () => {
    const tracker = new SnapshotRequestTracker();
    const requestId = tracker.create("task_1", "open");
    tracker.beginNavigationChange(true);

    expect(tracker.currentArchived()).toBe(true);
    expect(tracker.accept("task_1", requestId, "open")).toEqual({
      accepted: false,
      reason: "unknown_request",
    });
  });

  it("rejects stale open requests", () => {
    const tracker = new SnapshotRequestTracker();
    const first = tracker.create("task_1", "open");
    const second = tracker.create("task_2", "open");

    expect(tracker.accept("task_1", first, "open")).toMatchObject({
      accepted: false,
      reason: "stale_open_request",
      latestSnapshotRequestId: second,
    });
  });
});
