import { describe, expect, it } from "vitest";
import { appendQuoteToDraft, quoteableSelectionText } from "./quoteSelection";

describe("appendQuoteToDraft", () => {
  it("appends compact quoted lines without blank Composer rows", () => {
    expect(appendQuoteToDraft("Keep this draft.", "First line\n\nSecond line")).toBe(
      "Keep this draft.\n> First line\n>\n> Second line\n",
    );
  });

  it("requires three visible non-whitespace graphemes while preserving internal whitespace", () => {
    expect(quoteableSelectionText(" \n  One  \nTwo\n ")).toBe("One  \nTwo");
    expect(quoteableSelectionText(" a b ")).toBeUndefined();
    expect(quoteableSelectionText(" 👩🏽‍💻 a b ")).toBe("👩🏽‍💻 a b");
  });
});
