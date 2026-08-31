import { describe, expect, it } from "vitest";
import { desktopCommandForKeyboardEvent } from "./desktopCommands";

describe("Desktop keyboard commands", () => {
  it.each([
    ["n", "new-task"],
    ["o", "open-project"],
    [",", "settings"],
  ] as const)("keeps Ctrl+%s available without a duplicate app menu", (key, command) => {
    expect(desktopCommandForKeyboardEvent({ altKey: false, ctrlKey: true, key, metaKey: false, shiftKey: false })).toBe(command);
  });

  it("leaves modified and unrelated keys to the focused control", () => {
    expect(desktopCommandForKeyboardEvent({ altKey: false, ctrlKey: true, key: "n", metaKey: false, shiftKey: true })).toBeUndefined();
    expect(desktopCommandForKeyboardEvent({ altKey: false, ctrlKey: false, key: "n", metaKey: false, shiftKey: false })).toBeUndefined();
  });
});
