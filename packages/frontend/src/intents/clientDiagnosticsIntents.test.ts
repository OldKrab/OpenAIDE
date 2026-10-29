import { describe, expect, it, vi } from "vitest";
import { DIAGNOSTICS_RECORD_CLIENT_EVENTS } from "@openaide/app-server-client";
import { createClientDiagnosticsForwarder, protocolFields } from "./clientDiagnosticsIntents";

function harness(request = vi.fn(async () => ({ recorded: 1 }))) {
  const flushes: Array<() => void> = [];
  const onFailed = vi.fn();
  const forwarder = createClientDiagnosticsForwarder({ request } as never, {
    onFailed,
    now: () => 1_000,
    schedule: (flush) => { flushes.push(flush); },
  });
  return { flushes, forwarder, onFailed, request };
}

describe("client diagnostics forwarding", () => {
  it("sends a Task switch's events together in one request", async () => {
    const { flushes, forwarder, request } = harness();
    forwarder.record("task_route_open_started", { task_id: "task-a", generation: 2 });
    forwarder.record("task_route_open_completed", { task_id: "task-a", duration_ms: 274.6, accepted: true });
    expect(flushes).toHaveLength(1);
    flushes[0]();

    await vi.waitFor(() => expect(request).toHaveBeenCalledTimes(1));
    expect(request).toHaveBeenCalledWith(DIAGNOSTICS_RECORD_CLIENT_EVENTS, {
      events: [
        { event: "task_route_open_started", atMs: 1_000, fields: { task_id: "task-a", generation: 2 } },
        { event: "task_route_open_completed", atMs: 1_000, fields: { task_id: "task-a", duration_ms: 275, accepted: true } },
      ],
    });
  });

  it("stops forwarding after a refused batch and reports it once", async () => {
    const error = new Error("method not found");
    const { flushes, forwarder, onFailed, request } = harness(vi.fn(async () => { throw error; }));
    forwarder.record("task_state_baseline_ready", { task_id: "task-a" });
    flushes[0]();
    await vi.waitFor(() => expect(onFailed).toHaveBeenCalledWith(error));

    forwarder.record("task_state_baseline_ready", { task_id: "task-b" });
    expect(flushes).toHaveLength(1);
    expect(request).toHaveBeenCalledTimes(1);
  });

  it("keeps whole numbers, flags and words, and names an error by kind", () => {
    expect(protocolFields({ error_name: "TypeError", error_code: undefined, duration_ms: Number.NaN, generation: 3 }))
      .toEqual({ error_kind: "TypeError", generation: 3 });
  });
});
