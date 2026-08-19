import { describe, expect, it } from "vitest";
import { highlightFileViewerLines } from "./fileViewerHighlight";

describe("File Viewer highlighting", () => {
  it("leaves source uncolored when App Server has no language", () => {
    expect(highlightFileViewerLines("fn main() {}")).toEqual([[{ text: "fn main() {}" }]]);
  });

  it("leaves unknown extensions uncolored", () => {
    expect(highlightFileViewerLines("fn main() {}", "zzz")).toEqual([[{ text: "fn main() {}" }]]);
  });

  it("colors a rust keyword from the rs snapshot language", () => {
    const [line] = highlightFileViewerLines("fn main() {}", "rs");
    expect(line.some((span) => span.text === "fn" && Boolean(span.className))).toBe(true);
    expect(line.map((span) => span.text).join("")).toBe("fn main() {}");
  });

  it("keeps quoteable line text aligned with highlighted rows", () => {
    const text = "pub fn one() {}\n// two";
    const lines = text.split("\n");
    const highlighted = highlightFileViewerLines(text, "rs");
    expect(highlighted).toHaveLength(lines.length);
    expect(highlighted.map((spans) => spans.map((span) => span.text).join(""))).toEqual(lines);
  });
});
