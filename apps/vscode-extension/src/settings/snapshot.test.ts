import { afterEach, describe, expect, it, vi } from "vitest";
import {
  DEVELOPER_SETTINGS_UNLOCK_KEY,
  developerSettingsVisible,
  unlockDeveloperSettings,
} from "./snapshot";

describe("developer settings visibility", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it("is hidden by default", () => {
    expect(developerSettingsVisible()).toBe(false);
  });

  it("is visible when enabled by environment", () => {
    vi.stubEnv("OPENAIDE_DEVELOPER_SETTINGS", "1");

    expect(developerSettingsVisible()).toBe(true);
  });

  it("is visible when unlocked in persisted extension state", () => {
    const store = { get: vi.fn(() => true) };

    expect(developerSettingsVisible(store)).toBe(true);
    expect(store.get).toHaveBeenCalledWith(DEVELOPER_SETTINGS_UNLOCK_KEY, false);
  });

  it("persists unlock state", async () => {
    const store = { update: vi.fn(async () => undefined) };

    await unlockDeveloperSettings(store);

    expect(store.update).toHaveBeenCalledWith(DEVELOPER_SETTINGS_UNLOCK_KEY, true);
  });
});
