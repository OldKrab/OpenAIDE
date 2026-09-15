import { expect, it, vi } from "vitest";
import { registerBackNavigation } from "./useBackNavigation";

it("dismisses one topmost overlay before the drawer or settings", () => {
  const host = new EventTarget() as unknown as Window;
  const settings = vi.fn();
  const drawer = vi.fn();
  const dialog = vi.fn();
  const removeSettings = registerBackNavigation(host, settings, 20);
  const removeDrawer = registerBackNavigation(host, drawer, 60);
  const removeDialog = registerBackNavigation(host, dialog, 100);
  try {
    expect(host.dispatchEvent(new Event("openaide:back", { cancelable: true }))).toBe(false);
    expect(dialog).toHaveBeenCalledOnce();
    expect(drawer).not.toHaveBeenCalled();
    expect(settings).not.toHaveBeenCalled();
    removeDialog();
    host.dispatchEvent(new Event("openaide:back", { cancelable: true }));
    expect(drawer).toHaveBeenCalledOnce();
    removeDrawer();
    host.dispatchEvent(new Event("openaide:back", { cancelable: true }));
    expect(settings).toHaveBeenCalledOnce();
  } finally { removeSettings(); }
  expect(host.dispatchEvent(new Event("openaide:back", { cancelable: true }))).toBe(true);
});

it("closes only the most recently opened nested popup", () => {
  const host = new EventTarget() as unknown as Window;
  const parent = vi.fn();
  const child = vi.fn();
  const removeParent = registerBackNavigation(host, parent, 100);
  const removeChild = registerBackNavigation(host, child, 100);
  try {
    host.dispatchEvent(new Event("openaide:back", { cancelable: true }));
    expect(child).toHaveBeenCalledOnce();
    expect(parent).not.toHaveBeenCalled();
  } finally { removeChild(); removeParent(); }
});
