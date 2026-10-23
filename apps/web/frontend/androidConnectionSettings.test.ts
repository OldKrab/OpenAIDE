import { afterEach, describe, expect, it, vi } from "vitest";
import { androidConnectionSettings } from "./androidConnectionSettings";

afterEach(() => vi.useRealTimers());
function harness(userAgent = "Mozilla/5.0 OpenAIDE-Android/1") {
  const listeners = new Map<string, (event: any) => void>();
  const host = {
    navigator: { userAgent }, location: { origin: "http://localhost" },
    addEventListener: (name: string, listener: (event: any) => void) => listeners.set(name, listener),
    setTimeout, clearTimeout,
  } as unknown as Window;
  const port = { onmessage: undefined as undefined | ((event: any) => void), postMessage: vi.fn(), start: vi.fn(), close: vi.fn() };
  return {
    capability: androidConnectionSettings(host), port,
    attach: (origin = "http://localhost") => listeners.get("message")?.({ data: "openaide:connection-controls:1", ports: [port], origin }),
    receive: (message: unknown) => port.onmessage?.({ data: JSON.stringify(message) }),
    leave: () => listeners.get("pagehide")?.({}),
  };
}

describe("Android connection controls", () => {
  it("leaves ordinary browser Settings unchanged", () => {
    expect(harness("Mozilla/5.0 Android").capability).toBeUndefined();
  });
  it("waits for the native channel and correlates command acceptance", async () => {
    const test = harness();
    const accepted = test.capability!.execute({ action: "state" });
    expect(test.port.postMessage).not.toHaveBeenCalled();
    test.attach();
    const request = JSON.parse(test.port.postMessage.mock.calls[0][0]);
    expect(request.command).toEqual({ action: "state" });
    test.receive({ type: "result", id: request.id, accepted: true });
    await accepted;
  });
  it("subscribes to authoritative settings and replaces the snapshot on each update", () => {
    const test = harness();
    test.attach();
    const changed = vi.fn();
    const unsubscribe = test.capability!.subscribe(changed);
    test.receive({ type: "state", state: { remote: false, background: true } });
    test.receive({ type: "state", state: { remote: true, computer: "Studio", background: false } });
    expect(test.capability!.snapshot()).toMatchObject({ remote: true, computer: "Studio", background: false });
    expect(changed).toHaveBeenCalledTimes(2);
    unsubscribe();
  });
  it("rejects foreign handshakes and bounds unavailable settings", async () => {
    vi.useFakeTimers();
    const test = harness();
    test.attach("https://untrusted.example");
    const result = test.capability!.execute({ action: "state" });
    const rejected = expect(result).rejects.toThrow("unavailable");
    await vi.advanceTimersByTimeAsync(8000);
    await rejected;
    expect(test.port.postMessage).not.toHaveBeenCalled();
  });
  it("rejects pending actions on navigation without replaying them", async () => {
    const test = harness();
    test.attach();
    const result = test.capability!.execute({ action: "background", enabled: false });
    const rejected = expect(result).rejects.toThrow("unavailable");
    test.leave();
    await rejected;
    test.attach();
    expect(test.port.postMessage).toHaveBeenCalledTimes(1);
  });
  it("keeps the newest app update snapshot from the native channel", () => {
    const test = harness();
    test.attach();
    const updates = test.capability!.appUpdates;
    const changed = vi.fn();
    updates.subscribe(changed);
    test.receive({ type: "update", update: { revision: 2, installedVersion: "1.0.0", kind: "available", offer: { version: "1.1.0", notes: "", sizeBytes: 1 } } });
    test.receive({ type: "update", update: { revision: 1, installedVersion: "1.0.0", kind: "idle" } });
    expect(updates.snapshot()).toMatchObject({ revision: 2, kind: "available" });
    expect(changed).toHaveBeenCalledTimes(1);
    expect(updates.installation).toBe("systemInstaller");
  });
  it("forwards update taps to the app and reports the installer handoff as started", async () => {
    const test = harness();
    test.attach();
    const install = test.capability!.appUpdates.restartAndUpdate();
    const request = JSON.parse(test.port.postMessage.mock.calls[0][0]);
    expect(request.command).toEqual({ action: "update_install" });
    test.receive({ type: "result", id: request.id, accepted: true });
    await expect(install).resolves.toBe("started");
    void test.capability!.appUpdates.openReleaseNotes("1.1.0");
    expect(JSON.parse(test.port.postMessage.mock.calls[1][0]).command).toEqual({ action: "update_notes", version: "1.1.0" });
  });
  it("reports native route rejection", async () => {
    const test = harness();
    test.attach();
    const result = test.capability!.execute({ action: "check" });
    const request = JSON.parse(test.port.postMessage.mock.calls[0][0]);
    test.receive({ type: "result", id: request.id, accepted: false });
    await expect(result).rejects.toThrow("unavailable");
  });
  it("follows the phone's computers on every page, apart from the Settings state", () => {
    const test = harness();
    test.attach();
    const computers = test.capability!.computers;
    const changed = vi.fn();
    computers.subscribe(changed);
    expect(computers.snapshot()).toBeUndefined();
    test.receive({ type: "computers", computers: [{ id: "studio", name: "Studio", current: true }] });
    expect(computers.snapshot()).toEqual({ device: "phone", computers: [{ id: "studio", name: "Studio", current: true }] });
    expect(computers.snapshot()).toBe(computers.snapshot());
    expect(test.capability!.snapshot()).toBeUndefined();
    expect(changed).toHaveBeenCalledOnce();
  });
  it("switches through the phone's own commands and resolves with what the switch came to", async () => {
    const test = harness();
    test.attach();
    const sent = () => test.port.postMessage.mock.calls.map(([message]) => JSON.parse(message));
    const answer = (reply: Record<string, unknown>) => test.receive({ type: "result", id: sent().at(-1).id, ...reply });
    const toStudio = test.capability!.computers.use("studio", "computer-switch-1");
    answer({ accepted: true, outcome: "switched" });
    await expect(toStudio).resolves.toBe("switched");
    const toPhone = test.capability!.computers.use(undefined);
    answer({ accepted: true, outcome: "cancelled" });
    await expect(toPhone).resolves.toBe("cancelled");
    expect(sent().map((request) => request.command)).toEqual([
      { action: "use_computer", id: "studio", operationId: "computer-switch-1" },
      { action: "local" },
    ]);
  });
  it("keeps the phone's sentence when a computer cannot be reached", async () => {
    const test = harness();
    test.attach();
    const toStudio = test.capability!.computers.use("studio");
    const request = JSON.parse(test.port.postMessage.mock.calls[0][0]);
    test.receive({ type: "result", id: request.id, accepted: false, error: "Your computer is unreachable." });
    await expect(toStudio).rejects.toThrow("Your computer is unreachable.");
  });
  it("waits for a slow switch longer than for other commands, and cancels it", async () => {
    vi.useFakeTimers();
    const test = harness();
    test.attach();
    let settled = false;
    const toStudio = test.capability!.computers.use("studio").finally(() => { settled = true; });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(settled).toBe(false);
    void test.capability!.computers.cancel!();
    const [switching, cancelling] = test.port.postMessage.mock.calls.map(([message]) => JSON.parse(message));
    expect(cancelling.command).toEqual({ action: "cancel_switch" });
    test.receive({ type: "result", id: cancelling.id, accepted: true, outcome: "cancelled" });
    test.receive({ type: "result", id: switching.id, accepted: true, outcome: "cancelled" });
    await expect(toStudio).resolves.toBe("cancelled");
  });
  it("waits for forgetting the computer in use as for a switch, and keeps the phone's sentence", async () => {
    vi.useFakeTimers();
    const test = harness();
    test.attach();
    let settled = false;
    const forgotten = test.capability!.computers.forget("studio").finally(() => { settled = true; });
    const request = JSON.parse(test.port.postMessage.mock.calls[0][0]);
    expect(request.command).toEqual({ action: "forget_computer", id: "studio" });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(settled).toBe(false);
    test.receive({ type: "result", id: request.id, accepted: true, outcome: "switched" });
    await expect(forgotten).resolves.toBeUndefined();

    const failed = test.capability!.computers.forget("studio");
    const retried = JSON.parse(test.port.postMessage.mock.calls[1][0]);
    test.receive({ type: "result", id: retried.id, accepted: false, error: "Your local workspace could not start." });
    await expect(failed).rejects.toThrow("Your local workspace could not start.");
  });
});
