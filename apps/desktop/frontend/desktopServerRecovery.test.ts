import { describe, expect, it, vi } from "vitest";
import { createDesktopServerRecovery } from "./desktopServerRecovery";

function fakeTargets(visibilityState: DocumentVisibilityState = "visible") {
  const windowListeners = new Map<string, () => void>();
  const documentListeners = new Map<string, () => void>();
  const target = {
    addEventListener: (type: string, listener: () => void) => windowListeners.set(type, listener),
    removeEventListener: (type: string) => windowListeners.delete(type),
  } as unknown as Window;
  const documentTarget = {
    visibilityState,
    addEventListener: (type: string, listener: () => void) => documentListeners.set(type, listener),
    removeEventListener: (type: string) => documentListeners.delete(type),
  } as unknown as Document;
  return { documentListeners, documentTarget, target, windowListeners };
}

describe("desktop App Server recovery", () => {
  it("asks the host to restore the App Server when the window resumes", async () => {
    const { documentTarget, target, windowListeners } = fakeTargets();
    const invoke = vi.fn(async () => null);
    createDesktopServerRecovery({ invoke, target, documentTarget });

    windowListeners.get("focus")?.();
    await Promise.resolve();
    await Promise.resolve();

    expect(invoke).toHaveBeenCalledWith("desktop_ensure_app_server");
  });

  it("restores the App Server when the page becomes visible again", async () => {
    const { documentListeners, documentTarget, target } = fakeTargets();
    const invoke = vi.fn(async () => null);
    createDesktopServerRecovery({ invoke, target, documentTarget });

    documentListeners.get("visibilitychange")?.();
    await Promise.resolve();
    await Promise.resolve();

    expect(invoke).toHaveBeenCalledWith("desktop_ensure_app_server");
  });

  it("keeps one restore in flight at a time", async () => {
    const { target, documentTarget, windowListeners } = fakeTargets();
    let finish!: () => void;
    const invoke = vi.fn(() => new Promise<void>((resolve) => { finish = resolve; }));
    createDesktopServerRecovery({ invoke, target, documentTarget });

    windowListeners.get("focus")?.();
    windowListeners.get("pageshow")?.();

    expect(invoke).toHaveBeenCalledTimes(1);

    finish();
    await Promise.resolve();
    await Promise.resolve();
  });

  it("does not restore the App Server while the window is hidden", () => {
    const { target, documentTarget, windowListeners } = fakeTargets("hidden");
    const invoke = vi.fn(async () => null);
    createDesktopServerRecovery({ invoke, target, documentTarget });

    windowListeners.get("focus")?.();

    expect(invoke).not.toHaveBeenCalled();
  });

  it("stops listening when disposed", () => {
    const { target, documentTarget, windowListeners } = fakeTargets();
    const invoke = vi.fn(async () => null);
    const dispose = createDesktopServerRecovery({ invoke, target, documentTarget });

    dispose();

    expect(windowListeners.size).toBe(0);
  });
});
