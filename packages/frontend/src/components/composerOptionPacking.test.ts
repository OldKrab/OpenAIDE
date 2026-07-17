import { describe, expect, it } from "vitest";
import { visibleComposerOptionCount } from "./composerOptionPacking";

describe("composer option packing", () => {
  it("keeps the largest fitting prefix and groups the trailing options", () => {
    const optionWidths = [104, 136, 112, 82];
    const overflowWidths = [0, 76, 76, 76, 76];

    expect(visibleComposerOptionCount({
      availableWidth: 470,
      gap: 4,
      optionWidths,
      overflowWidths,
    })).toBe(4);
    expect(visibleComposerOptionCount({
      availableWidth: 340,
      gap: 4,
      optionWidths,
      overflowWidths,
    })).toBe(2);
    expect(visibleComposerOptionCount({
      availableWidth: 190,
      gap: 4,
      optionWidths,
      overflowWidths,
    })).toBe(1);
    expect(visibleComposerOptionCount({
      availableWidth: 76,
      gap: 4,
      optionWidths,
      overflowWidths,
    })).toBe(0);
  });

  it("reserves the width of the group control for the actual hidden count", () => {
    expect(visibleComposerOptionCount({
      availableWidth: 184,
      gap: 4,
      optionWidths: [100, 100],
      overflowWidths: [0, 70, 80],
    })).toBe(1);
    expect(visibleComposerOptionCount({
      availableWidth: 173,
      gap: 4,
      optionWidths: [100, 100],
      overflowWidths: [0, 70, 80],
    })).toBe(0);
  });
});
