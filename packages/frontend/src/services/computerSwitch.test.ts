// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ComputerSwitchResult, ComputersCapability } from "./computers";
import { computerSwitch, reportComputerSwitchArrival } from "./computerSwitch";
import { takeKeptDrafts } from "./keptDrafts";
import { reportDraftTexts, reportUnsentInput } from "./unsentInput";

function capability(use: ComputersCapability["use"], cancel?: () => Promise<void>): ComputersCapability {
  return {
    snapshot: () => ({ device: "computer", computers: [{ id: "studio", name: "Studio", current: false }] }),
    subscribe: () => () => undefined,
    use,
    cancel,
    forget: async () => undefined,
  };
}

function deferred() {
  let settle!: (result: ComputerSwitchResult) => void;
  let refuse!: (cause: unknown) => void;
  const promise = new Promise<ComputerSwitchResult>((resolve, reject) => { settle = resolve; refuse = reject; });
  return { promise, settle, refuse };
}

describe("computer switch", () => {
  beforeEach(() => {
    vi.spyOn(console, "info").mockImplementation(() => undefined);
    vi.spyOn(console, "warn").mockImplementation(() => undefined);
  });
  afterEach(() => {
    vi.restoreAllMocks();
    sessionStorage.clear();
    localStorage.clear();
  });

  it("is one switch per shell, shared by every place that offers it", () => {
    const shell = capability(async () => "switched");
    expect(computerSwitch(shell)).toBe(computerSwitch(shell));
  });

  it("stays pending once the shell replaces the page", async () => {
    const use = vi.fn(async () => "switched" as const);
    const store = computerSwitch(capability(use));
    await store.request("studio");
    expect(use).toHaveBeenCalledWith("studio", expect.stringMatching(/^computer-switch-/));
    expect(store.snapshot()).toEqual({ pending: "studio", cancellable: false });
  });

  it("asks for nothing while a switch is connecting", async () => {
    const answer = deferred();
    const use = vi.fn(() => answer.promise);
    const store = computerSwitch(capability(use));
    void store.request("studio");
    void store.request();
    expect(use).toHaveBeenCalledOnce();
    answer.settle("switched");
  });

  it("keeps the user in place with the shell's sentence when the computer cannot be reached", async () => {
    const store = computerSwitch(capability(async () => { throw new Error("Studio is unreachable."); }));
    const changed = vi.fn();
    store.subscribe(changed);
    await store.request("studio");
    expect(store.snapshot()).toEqual({ cancellable: false, failed: { id: "studio", message: "Studio is unreachable." } });
    expect(changed).toHaveBeenCalled();
  });

  it("cancels a switch still connecting and stays put", async () => {
    const answer = deferred();
    const cancel = vi.fn(async () => { answer.settle("cancelled"); });
    const store = computerSwitch(capability(() => answer.promise, cancel));
    const requested = store.request("studio");
    expect(store.snapshot()).toEqual({ pending: "studio", cancellable: true });
    store.cancel();
    await requested;
    expect(cancel).toHaveBeenCalledOnce();
    expect(store.snapshot()).toEqual({ cancellable: false });
  });

  it("returns to rest when the chosen computer was already in use", async () => {
    const store = computerSwitch(capability(async () => "unchanged"));
    await store.request();
    expect(store.snapshot()).toEqual({ cancellable: false });
  });

  it("clears an earlier failure with the next attempt", async () => {
    let fail = true;
    const store = computerSwitch(capability(async () => {
      if (fail) throw new Error("Studio is unreachable.");
      return "switched";
    }));
    await store.request("studio");
    fail = false;
    await store.request("studio");
    expect(store.snapshot().failed).toBeUndefined();
  });

  describe("with draft text", () => {
    let stopReporting: () => void;
    beforeEach(() => { stopReporting = reportDraftTexts(() => ({ tasks: { "task-1": "half a thought" } })); });
    afterEach(() => stopReporting());

    it("switches without asking and keeps the text for the computer it leaves", async () => {
      const use = vi.fn(async () => "switched" as const);
      const store = computerSwitch(capability(use));
      await store.request("studio");
      expect(use).toHaveBeenCalledOnce();
      expect(takeKeptDrafts("")).toEqual({ tasks: { "task-1": "half a thought" } });
    });

    it("has the text kept before the shell may replace the page", async () => {
      let keptDuringUse: unknown;
      const store = computerSwitch(capability(async () => {
        keptDuringUse = takeKeptDrafts("");
        return "switched";
      }));
      await store.request("studio");
      expect(keptDuringUse).toEqual({ tasks: { "task-1": "half a thought" } });
    });

    it.each([
      ["refused", async () => { throw new Error("Studio is unreachable."); }],
      ["cancelled", async () => "cancelled" as const],
      ["unchanged", async () => "unchanged" as const],
    ])("drops the copy when the switch is %s, since the page stays", async (_ending, use) => {
      await computerSwitch(capability(use)).request("studio");
      expect(takeKeptDrafts("")).toBeUndefined();
    });

    it("drops the copy when the user stays after being asked about an attachment", async () => {
      const stopProbe = reportUnsentInput(() => true);
      const store = computerSwitch(capability(async () => "switched"));
      await store.request("studio");
      store.decline();
      stopProbe();
      expect(takeKeptDrafts("")).toBeUndefined();
    });

    it("asks when device storage refuses the text, and says the text is at risk", async () => {
      const storageFull = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new DOMException("full", "QuotaExceededError"); });
      const use = vi.fn(async () => "switched" as const);
      const store = computerSwitch(capability(use));
      await store.request("studio");
      storageFull.mockRestore();
      expect(use).not.toHaveBeenCalled();
      expect(store.snapshot()).toEqual({ confirming: "studio", cancellable: false, draftsAtRisk: true });
      expect(console.info).toHaveBeenCalledWith(expect.stringContaining("computer_switch_drafts_kept"));
      expect(vi.mocked(console.info).mock.calls.flat().join()).toContain("outcome=storage_failed");
      expect(vi.mocked(console.info).mock.calls.flat().join()).not.toContain("half a thought");
    });
  });

  describe("with unsent input", () => {
    let stopReporting: () => void;
    beforeEach(() => { stopReporting = reportUnsentInput(() => true); });
    afterEach(() => stopReporting());

    it("asks before connecting and connects once the user agrees", async () => {
      const use = vi.fn(async () => "switched" as const);
      const store = computerSwitch(capability(use));
      await store.request("studio");
      expect(use).not.toHaveBeenCalled();
      expect(store.snapshot()).toEqual({ confirming: "studio", cancellable: false });
      await store.confirm();
      expect(use).toHaveBeenCalledWith("studio", expect.stringMatching(/^computer-switch-/));
      expect(store.snapshot()).toEqual({ pending: "studio", cancellable: false });
    });

    it("stays put and connects nothing when the user keeps the input", async () => {
      const use = vi.fn(async () => "switched" as const);
      const store = computerSwitch(capability(use));
      await store.request();
      store.decline();
      expect(use).not.toHaveBeenCalled();
      expect(store.snapshot()).toEqual({ cancellable: false });
      expect(console.info).toHaveBeenLastCalledWith(expect.stringContaining("outcome=declined"));
    });
  });

  it("asks nothing once the input is gone", async () => {
    const stopReporting = reportUnsentInput(() => true);
    stopReporting();
    const use = vi.fn(async () => "unchanged" as const);
    await computerSwitch(capability(use)).request("studio");
    expect(use).toHaveBeenCalledOnce();
  });

  it("closes the diagnostics of a switch on the page that replaced it", async () => {
    await computerSwitch(capability(async () => "switched")).request("studio");
    const operationId = (vi.mocked(console.info).mock.calls[0][0] as string).match(/operation_id=(\S+)/)![1];
    reportComputerSwitchArrival();
    expect(console.info).toHaveBeenLastCalledWith(expect.stringMatching(new RegExp(`^computer_switch_ready operation_id=${operationId} duration_ms=\\d+$`)));
    vi.mocked(console.info).mockClear();
    reportComputerSwitchArrival();
    expect(console.info).not.toHaveBeenCalled();
  });
});
