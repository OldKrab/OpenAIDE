// @vitest-environment jsdom

import { afterEach, expect, it, vi } from "vitest";
import { androidSystemAppearance } from "./androidSystemAppearance";

afterEach(() => {
  vi.restoreAllMocks();
  delete document.body.dataset.theme;
  document.body.style.removeProperty("--oa-panel");
});

it("sends the rendered opaque color, follows app theme changes, and stays quiet otherwise", async () => {
  let channels = new Uint8ClampedArray([248, 249, 251, 255]);
  const fillRect = vi.fn();
  const context = {
    clearRect: vi.fn(), fillRect, getImageData: () => ({ data: channels }), fillStyle: "",
  };
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(context as unknown as CanvasRenderingContext2D);
  document.body.style.setProperty("--oa-panel", "rgb(238, 240, 243)");
  const send = vi.fn();
  const appearance = androidSystemAppearance(window, send);
  try {
    appearance.refresh();
    expect(context.fillStyle).toBe("rgb(238, 240, 243)");
    expect(send).toHaveBeenLastCalledWith("#f8f9fb");
    document.body.dataset.theme = "light";
    await Promise.resolve();
    expect(send).toHaveBeenCalledTimes(1);
    channels = new Uint8ClampedArray([31, 31, 31, 255]);
    document.body.dataset.theme = "dark";
    await Promise.resolve();
    expect(send).toHaveBeenLastCalledWith("#1f1f1f");
    const reads = fillRect.mock.calls.length;
    document.body.dataset.unrelated = "change";
    await Promise.resolve();
    expect(fillRect).toHaveBeenCalledTimes(reads);
    appearance.refresh();
    expect(send).toHaveBeenCalledTimes(3);
    appearance.dispose();
    channels = new Uint8ClampedArray([248, 249, 251, 255]);
    document.body.dataset.theme = "light";
    await Promise.resolve();
    expect(send).toHaveBeenCalledTimes(3);
    appearance.refresh();
    expect(send).toHaveBeenLastCalledWith("#f8f9fb");
  } finally {
    appearance.dispose();
    delete document.body.dataset.unrelated;
  }
});

it("does not send transparent colors or fail without a canvas context", () => {
  const canvas = vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
    clearRect: vi.fn(), fillRect: vi.fn(), getImageData: () => ({ data: new Uint8ClampedArray([0, 0, 0, 0]) }),
  } as unknown as CanvasRenderingContext2D);
  const send = vi.fn();
  const appearance = androidSystemAppearance(window, send);
  appearance.refresh();
  appearance.dispose();
  expect(send).not.toHaveBeenCalled();
  canvas.mockReturnValue(null);
  const unavailable = androidSystemAppearance(window, send);
  expect(() => unavailable.refresh()).not.toThrow();
  unavailable.dispose();
  expect(send).not.toHaveBeenCalled();
});
