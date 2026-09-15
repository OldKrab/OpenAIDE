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
  it("subscribes to authoritative settings and preserves QR results across updates", () => {
    const test = harness();
    test.attach();
    const changed = vi.fn();
    const unsubscribe = test.capability!.subscribe(changed);
    test.receive({ type: "state", state: { remote: false, background: true } });
    test.receive({ type: "scanned", address: "https://computer.example" });
    test.receive({ type: "state", state: { remote: false, background: false } });
    expect(test.capability!.snapshot()).toMatchObject({ background: false, scannedAddress: "https://computer.example", scanSequence: 1 });
    expect(changed).toHaveBeenCalledTimes(3);
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
  it("reports native route rejection", async () => {
    const test = harness();
    test.attach();
    const result = test.capability!.execute({ action: "check" });
    const request = JSON.parse(test.port.postMessage.mock.calls[0][0]);
    test.receive({ type: "result", id: request.id, accepted: false });
    await expect(result).rejects.toThrow("unavailable");
  });
});
