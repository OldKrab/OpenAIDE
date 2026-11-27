import { describe, expect, it, vi } from "vitest";
import type { InitializeResult } from "./generated/protocol.js";
import { createGlobalStateSync } from "./globalStateSync.js";

const logger = { info: vi.fn(), warn: vi.fn(), error: vi.fn() };

function result(remoteAccess: "off" | "on" = "off"): InitializeResult {
  return {
    snapshot: {
      global: {
        projects: { projects: [] },
        worktrees: [],
        agents: { agents: [] },
        settings: { sections: [] },
        newTaskDefaults: {},
        devices: { remoteAccess, serverName: "workstation" },
        terminals: { terminals: [] },
      },
    },
  } as unknown as InitializeResult;
}

const on = { devices: { remoteAccess: "on" as const, serverName: "workstation" } };

describe("createGlobalStateSync", () => {
  it("merges changes received before the reply is installed into that reply", () => {
    const sync = createGlobalStateSync(logger);
    const listener = vi.fn();
    sync.subscribe(listener);

    sync.receive(on);
    expect(listener).not.toHaveBeenCalled();
    const installed = sync.install(result());

    expect(installed.snapshot.global.devices.remoteAccess).toBe("on");
    expect(sync.latest()).toBe(installed);
  });

  it("applies later changes to the held result and forwards them", () => {
    const sync = createGlobalStateSync(logger);
    const listener = vi.fn();
    sync.subscribe(listener);
    sync.install(result());

    sync.receive(on);

    expect(listener).toHaveBeenCalledWith(on);
    expect(sync.latest()?.snapshot.global.devices.remoteAccess).toBe("on");
  });

  it("drops the held state on suspend and buffers changes for the recovery baseline", () => {
    const sync = createGlobalStateSync(logger);
    sync.install(result());
    sync.suspend();
    expect(sync.latest()).toBeUndefined();

    sync.receive(on);
    const baseline = sync.install(result());

    expect(baseline.snapshot.global.devices.remoteAccess).toBe("on");
  });

  it("keeps notifying when a listener throws", () => {
    const sync = createGlobalStateSync(logger);
    const second = vi.fn();
    sync.subscribe(() => { throw new Error("boom"); });
    sync.subscribe(second);
    sync.install(result());

    sync.receive(on);

    expect(second).toHaveBeenCalledOnce();
  });
});
