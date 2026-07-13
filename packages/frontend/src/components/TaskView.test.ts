import { describe, expect, it } from "vitest";
import { scrollTopAfterPrependedContent } from "./TaskViewModel";

describe("TaskView presentation", () => {
  it("keeps the same visible content anchored after earlier messages prepend", () => {
    expect(scrollTopAfterPrependedContent({
      previousScrollHeight: 1000,
      previousScrollTop: 240,
      nextScrollHeight: 1380,
    })).toBe(620);
  });
});
