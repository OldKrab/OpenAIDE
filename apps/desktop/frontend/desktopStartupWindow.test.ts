// @vitest-environment jsdom
import startupHtml from "../index.html?raw";
import { afterEach, expect, it, vi } from "vitest";

const nativeWindow = vi.hoisted(() => ({
  isDecorated: vi.fn(async () => false),
  startDragging: vi.fn(async () => undefined),
  minimize: vi.fn(async () => undefined),
  toggleMaximize: vi.fn(async () => undefined),
  close: vi.fn(async () => undefined),
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => nativeWindow }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(() => new Promise(() => {})) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("../../../packages/frontend/src/startFrontend", () => ({ startFrontend: vi.fn() }));
vi.mock("./desktopShell", () => ({ createDesktopShell: vi.fn() }));

afterEach(() => { vi.useRealTimers(); vi.clearAllMocks(); vi.resetModules(); });

it("can drag and use caption buttons while backend startup is pending", async () => {
  vi.useFakeTimers();
  document.documentElement.innerHTML = startupHtml;
  await import("./main");
  const header = document.querySelector<HTMLElement>('[aria-label="Desktop window controls"]');
  expect(header).not.toBeNull();
  expect(header!.hidden).toBe(false);
  header!.dispatchEvent(new MouseEvent("mousedown", { button: 0, detail: 1, bubbles: true }));
  expect(nativeWindow.startDragging).toHaveBeenCalledOnce();
  header!.dispatchEvent(new MouseEvent("mousedown", { button: 2, detail: 1, bubbles: true }));
  expect(nativeWindow.startDragging).toHaveBeenCalledOnce();
  for (const [label, method] of [["Minimize", "minimize"], ["Maximize or restore", "toggleMaximize"], ["Close", "close"]] as const) {
    const button = header!.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!;
    expect(button).not.toBeNull();
    button.dispatchEvent(new MouseEvent("mousedown", { button: 0, detail: 1, bubbles: true }));
    button.click();
    expect(nativeWindow[method]).toHaveBeenCalledOnce();
  }
  expect(nativeWindow.startDragging).toHaveBeenCalledOnce();
  header!.dispatchEvent(new MouseEvent("dblclick", { button: 0, detail: 2, bubbles: true }));
  expect(nativeWindow.toggleMaximize).toHaveBeenCalledTimes(2);
});

it("keeps native window controls on decorated platforms", async () => {
  vi.useFakeTimers();
  nativeWindow.isDecorated.mockResolvedValueOnce(true);
  document.documentElement.innerHTML = startupHtml;
  await import("./main");
  expect(document.querySelector<HTMLElement>('[aria-label="Desktop window controls"]')!.hidden).toBe(true);
});

it("keeps close available after startup fails", async () => {
  vi.useFakeTimers();
  const { invoke } = await import("@tauri-apps/api/core");
  vi.mocked(invoke).mockRejectedValueOnce(new Error("Startup unavailable"));
  document.documentElement.innerHTML = startupHtml;
  await import("./main");
  await vi.waitFor(() => expect(document.querySelector(".desktop-startup-label")!.textContent).toContain("could not start"));
  const header = document.querySelector<HTMLElement>('[aria-label="Desktop window controls"]')!;
  expect(header.hidden).toBe(false);
  header.querySelector<HTMLButtonElement>('[aria-label="Close"]')!.click();
  expect(nativeWindow.close).toHaveBeenCalledOnce();
});
