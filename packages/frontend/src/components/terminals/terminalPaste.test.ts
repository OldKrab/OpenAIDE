import { describe, expect, it } from "vitest";
import { leavesPasteToBrowser } from "./terminalPaste";

const key = (key: string, modifiers: Partial<Record<"ctrlKey" | "shiftKey" | "altKey" | "metaKey", boolean>> = {}) => ({
  key, ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, ...modifiers,
});

describe("leavesPasteToBrowser", () => {
  it("lets the browser paste on Ctrl+V outside macOS", () => {
    expect(leavesPasteToBrowser(key("v", { ctrlKey: true }), "Linux x86_64")).toBe(true);
    expect(leavesPasteToBrowser(key("V", { ctrlKey: true }), "Win32")).toBe(true);
  });

  it("keeps other control keys for the program in the terminal", () => {
    expect(leavesPasteToBrowser(key("c", { ctrlKey: true }), "Linux x86_64")).toBe(false);
    expect(leavesPasteToBrowser(key("v"), "Linux x86_64")).toBe(false);
    // Ctrl+Shift+V and Alt combinations keep their existing meaning.
    expect(leavesPasteToBrowser(key("V", { ctrlKey: true, shiftKey: true }), "Linux x86_64")).toBe(false);
    expect(leavesPasteToBrowser(key("v", { ctrlKey: true, altKey: true }), "Linux x86_64")).toBe(false);
  });

  it("keeps Ctrl+V as a control key on macOS, which pastes with Cmd+V", () => {
    expect(leavesPasteToBrowser(key("v", { ctrlKey: true }), "MacIntel")).toBe(false);
  });
});
