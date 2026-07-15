import { describe, expect, it } from "vitest";
import {
  fileMentionRanges,
  fileMentionTokenAtCursor,
  replaceFileMention,
} from "./ComposerFileMentions";

describe("workspace file mentions", () => {
  it("opens completion only for @ at the start or after whitespace", () => {
    expect(fileMentionTokenAtCursor("@src/ma", 7)).toEqual({ start: 0, end: 7, query: "src/ma" });
    expect(fileMentionTokenAtCursor("open @src/ma", 12)).toEqual({ start: 5, end: 12, query: "src/ma" });
    expect(fileMentionTokenAtCursor("email@example.com", 17)).toBeUndefined();
    expect(fileMentionTokenAtCursor("open @src main", 14)).toBeUndefined();
  });

  it("supports quoted searches and stops after a closing quote", () => {
    expect(fileMentionTokenAtCursor('use @"docs/work', 15)).toEqual({ start: 4, end: 15, query: "docs/work" });
    expect(fileMentionTokenAtCursor('use @"docs/work tree.md"', 24)).toBeUndefined();
  });

  it("inserts plain prompt text and quotes paths containing whitespace", () => {
    expect(replaceFileMention("open @doc", { start: 5, end: 9, query: "doc" }, "docs/task.md"))
      .toEqual({ text: "open @docs/task.md ", cursor: 19 });
    expect(replaceFileMention("@slides", { start: 0, end: 7, query: "slides" }, "docs/team deck.pptx"))
      .toEqual({ text: '@"docs/team deck.pptx" ', cursor: 23 });
  });

  it("finds syntax-only mentions for composer and chat rendering", () => {
    expect(fileMentionRanges('Read @src/main.rs and @"docs/team deck.pptx".')).toEqual([
      { start: 5, end: 17 },
      { start: 22, end: 44 },
    ]);
  });
});
