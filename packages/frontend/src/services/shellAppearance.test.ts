import { describe, expect, it, vi } from "vitest";
import { createShellAppearance } from "./shellAppearance";

describe("shell appearance", () => {
  it("persists a fixed theme and applies it immediately", () => {
    const body = { dataset: {} as DOMStringMap };
    const storage = { getItem: vi.fn(() => null), setItem: vi.fn() };
    const appearance = createShellAppearance({
      body,
      storage,
      storageKey: "test.theme",
      systemTheme: { matches: true, addEventListener: vi.fn() },
    });

    expect(appearance.theme()).toBe("system");
    expect(body.dataset.theme).toBe("dark");

    appearance.setTheme("light");

    expect(storage.setItem).toHaveBeenCalledWith("test.theme", "light");
    expect(appearance.theme()).toBe("light");
    expect(body.dataset.theme).toBe("light");
  });

  it("follows system changes and synchronized shell storage", () => {
    const body = { dataset: {} as DOMStringMap };
    let onSystemChange: () => void = () => undefined;
    let onStoredTheme: (value: string | null) => void = () => undefined;
    const systemTheme = {
      matches: false,
      addEventListener: (_type: "change", listener: () => void) => { onSystemChange = listener; },
    };
    const appearance = createShellAppearance({
      body,
      storage: { getItem: () => "invalid", setItem: vi.fn() },
      storageKey: "test.theme",
      systemTheme,
      subscribeStoredTheme: (listener) => { onStoredTheme = listener; },
    });

    expect(body.dataset.theme).toBe("light");
    systemTheme.matches = true;
    onSystemChange();
    expect(body.dataset.theme).toBe("dark");

    onStoredTheme("light");
    expect(appearance.theme()).toBe("light");
    expect(body.dataset.theme).toBe("light");
  });

  it("publishes the resolved theme so a native shell can match its backing surface", () => {
    const body = { dataset: {} as DOMStringMap };
    const onResolvedThemeChange = vi.fn();
    let onSystemChange: () => void = () => undefined;
    const systemTheme = {
      matches: false,
      addEventListener: (_type: "change", listener: () => void) => { onSystemChange = listener; },
    };
    const appearance = createShellAppearance({
      body,
      onResolvedThemeChange,
      storage: { getItem: () => "system", setItem: vi.fn() },
      storageKey: "test.theme",
      systemTheme,
    });

    expect(onResolvedThemeChange).toHaveBeenLastCalledWith("light");
    systemTheme.matches = true;
    onSystemChange();
    expect(onResolvedThemeChange).toHaveBeenLastCalledWith("dark");

    appearance.setTheme("light");
    expect(onResolvedThemeChange).toHaveBeenLastCalledWith("light");
  });
});
