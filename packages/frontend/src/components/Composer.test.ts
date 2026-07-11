import { describe, expect, it } from "vitest";
import { shouldInsertComposerNewline, shouldSubmitComposerKey } from "./Composer";

describe("composer submit shortcut", () => {
  it("uses Ctrl or Cmd Enter by default", () => {
    expect(shouldSubmitComposerKey(key({ ctrlKey: true }), "mod_enter")).toBe(true);
    expect(shouldSubmitComposerKey(key({ metaKey: true }), "mod_enter")).toBe(true);
    expect(shouldSubmitComposerKey(key(), "mod_enter")).toBe(false);
  });

  it("swaps Enter and modifier Enter when Enter sends", () => {
    expect(shouldSubmitComposerKey(key(), "enter")).toBe(true);
    expect(shouldSubmitComposerKey(key({ ctrlKey: true }), "enter")).toBe(false);
    expect(shouldSubmitComposerKey(key({ metaKey: true }), "enter")).toBe(false);
  });

  it("keeps Shift Enter and composing text as textarea input", () => {
    expect(shouldSubmitComposerKey(key({ shiftKey: true }), "enter")).toBe(false);
    expect(shouldSubmitComposerKey(key({ nativeEvent: { isComposing: true } }), "enter")).toBe(false);
  });
});

describe("composer newline shortcut", () => {
  it("uses Shift Enter in both shortcut modes", () => {
    expect(shouldInsertComposerNewline(key({ shiftKey: true }), "mod_enter")).toBe(true);
    expect(shouldInsertComposerNewline(key({ shiftKey: true }), "enter")).toBe(true);
  });

  it("uses Ctrl or Cmd Enter as newline when Enter sends", () => {
    expect(shouldInsertComposerNewline(key({ ctrlKey: true }), "enter")).toBe(true);
    expect(shouldInsertComposerNewline(key({ metaKey: true }), "enter")).toBe(true);
    expect(shouldInsertComposerNewline(key({ ctrlKey: true }), "mod_enter")).toBe(false);
    expect(shouldInsertComposerNewline(key({ metaKey: true }), "mod_enter")).toBe(false);
  });

  it("ignores composing and alternate shortcut events", () => {
    expect(shouldInsertComposerNewline(key({ altKey: true, ctrlKey: true }), "enter")).toBe(false);
    expect(shouldInsertComposerNewline(key({ key: "A", shiftKey: true }), "enter")).toBe(false);
    expect(shouldInsertComposerNewline(key({ nativeEvent: { isComposing: true }, shiftKey: true }), "enter")).toBe(false);
  });
});

function key(overrides: Partial<Parameters<typeof shouldSubmitComposerKey>[0]> = {}) {
  return {
    altKey: false,
    ctrlKey: false,
    key: "Enter",
    metaKey: false,
    nativeEvent: { isComposing: false },
    shiftKey: false,
    ...overrides,
  };
}
