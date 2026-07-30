import { describe, expect, it } from "vitest";
import { mapProtocolAppPreferences } from "./appPreferencesMapping";

describe("App preference mapping", () => {
  it("maps the persisted theme with the composer shortcut", () => {
    expect(mapProtocolAppPreferences({
      preferences: {
        composerSubmitShortcut: "modEnter",
        theme: "light",
      },
    })).toEqual({
      composer_submit_shortcut: "mod_enter",
      theme: "light",
    });
  });
});
