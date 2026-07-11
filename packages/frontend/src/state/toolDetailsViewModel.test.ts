import { describe, expect, it } from "vitest";
import type { ActivityStep, ActivityToolDetails } from "@openaide/app-shell-contracts";
import {
  editDiffLines,
  editResultText,
  executeDetailInfo,
  firstToolPath,
  buildUnifiedDiff,
  openablePath,
  parseSearchResults,
  searchDetailInfo,
} from "./toolDetailsViewModel";

describe("toolDetailsViewModel", () => {
  it("parses search output with optional column numbers", () => {
    expect(parseSearchResults("src/a.ts:7:alpha\nsrc/b.ts:9:2:beta", "/workspace")).toEqual([
      { displayPath: "src/a.ts", line: 7, path: "/workspace/src/a.ts", text: "alpha" },
      { displayPath: "src/b.ts", line: 9, path: "/workspace/src/b.ts", text: "beta" },
    ]);
  });

  it("returns file search results as openable paths against cwd", () => {
    const info = searchDetailInfo(
      details({
        input: input({ command: ["rg", "--files", "-g", "*.ts"], cwd: "/workspace" }),
        output: output({ stdout: "src/a.ts\nsrc/b.ts", exit_code: 0 }),
      }),
      toolStep({ input_summary: "Find files" }),
    );

    expect(info.mode).toBe("files");
    expect(info.fileResults).toEqual([
      { displayPath: "src/a.ts", path: "/workspace/src/a.ts" },
      { displayPath: "src/b.ts", path: "/workspace/src/b.ts" },
    ]);
    expect(openablePath("src/a.ts", "/workspace")).toBe("/workspace/src/a.ts");
  });

  it("classifies failed execute output using stderr before other output", () => {
    const info = executeDetailInfo(
      details({
        output: output({
          stderr: "permission denied",
          aggregated_output: "aggregate",
          formatted_output: "formatted",
          stdout: "stdout",
          exit_code: 1,
        }),
      }),
      toolStep({ name: "execute", status: "completed" }),
      "fallback",
    );

    expect(info.mode).toBe("failed");
    expect(info.outputLabel).toBe("stderr");
    expect(info.outputText).toBe("permission denied");
  });

  it("preserves edit result text for created, updated, and failed edits", () => {
    const created = details({ content: [{ kind: "diff", path: "/workspace/new.md", new_text: "new" }] });
    const updated = details({
      content: [{ kind: "diff", path: "/workspace/existing.md", old_text: "old", new_text: "new" }],
    });
    const failed = details({ output: output({ stderr: "edit failed", success: false }) });

    expect(editResultText(created, "/workspace/new.md", false)).toBe("Created workspace/new.md");
    expect(editResultText(updated, "/workspace/existing.md", false)).toBe("Updated workspace/existing.md");
    expect(editResultText(failed, "/workspace/fail.md", true, "fallback")).toBe("edit failed");
  });

  it("caps rendered edit diff rows for large file changes", () => {
    const lines = editDiffLines({
      kind: "diff",
      path: "/workspace/large.ts",
      old_text: undefined,
      new_text: Array.from({ length: 1_000 }, (_, index) => `line ${index + 1}`).join("\n"),
    });

    expect(lines).toHaveLength(401);
    expect(lines.at(-1)).toEqual({
      kind: "omitted",
      count: 601,
      text: "Diff truncated. Open the file for the full change.",
    });
  });

  it("renders rewritten modified files as replacements instead of a deletion block", () => {
    expect(buildUnifiedDiff("import old\nconst name = 'old';", "import updated\nconst name = 'updated';")).toEqual([
      { kind: "remove", text: "import old" },
      { kind: "add", text: "import updated" },
      { kind: "remove", text: "const name = 'old';" },
      { kind: "add", text: "const name = 'updated';" },
    ]);
  });

  it("avoids building full line lists for huge modified diffs", () => {
    const oldText = Array.from({ length: 1_000 }, (_, index) => `old ${index + 1}`).join("\n");
    const newText = Array.from({ length: 1_000 }, (_, index) => `new ${index + 1}`).join("\n");

    expect(buildUnifiedDiff(oldText, newText)).toHaveLength(401);
    expect(buildUnifiedDiff(oldText, newText).at(-1)).toEqual({
      kind: "context",
      text: "Diff truncated. Open the file for the full change.",
    });
  });

  it("renders only the changed hunk and nearby context for a large file", () => {
    const leadingContext = Array.from({ length: 105 }, (_, index) => `leading ${index + 1}`);
    const trailingContext = Array.from({ length: 105 }, (_, index) => `trailing ${index + 1}`);
    const oldText = [...leadingContext, "before", ...trailingContext].join("\n");
    const newText = [...leadingContext, "after", ...trailingContext].join("\n");

    expect(editDiffLines({ kind: "diff", path: "/workspace/large.ts", old_text: oldText, new_text: newText })).toEqual([
      { kind: "omitted", count: 102, text: "102 unchanged lines" },
      { kind: "hunk", oldStart: 103, oldCount: 7, newStart: 103, newCount: 7, text: "@@ -103,7 +103,7 @@" },
      { kind: "context", oldLineNumber: 103, newLineNumber: 103, prefix: " ", text: "leading 103" },
      { kind: "context", oldLineNumber: 104, newLineNumber: 104, prefix: " ", text: "leading 104" },
      { kind: "context", oldLineNumber: 105, newLineNumber: 105, prefix: " ", text: "leading 105" },
      { kind: "remove", oldLineNumber: 106, prefix: "-", text: "before" },
      { kind: "add", newLineNumber: 106, prefix: "+", text: "after" },
      { kind: "context", oldLineNumber: 107, newLineNumber: 107, prefix: " ", text: "trailing 1" },
      { kind: "context", oldLineNumber: 108, newLineNumber: 108, prefix: " ", text: "trailing 2" },
      { kind: "context", oldLineNumber: 109, newLineNumber: 109, prefix: " ", text: "trailing 3" },
      { kind: "omitted", count: 102, text: "102 unchanged lines" },
    ]);
  });

  it("separates distant changes in a large file into distinct hunks", () => {
    const oldLines = Array.from({ length: 300 }, (_, index) => `line ${index + 1}`);
    const newLines = [...oldLines];
    newLines[49] = "changed 50";
    newLines[249] = "changed 250";

    const rows = editDiffLines({
      kind: "diff",
      path: "/workspace/large.ts",
      old_text: oldLines.join("\n"),
      new_text: newLines.join("\n"),
    });

    expect(rows.filter((row) => row.kind === "hunk").map((row) => row.text)).toEqual([
      "@@ -47,7 +47,7 @@",
      "@@ -247,7 +247,7 @@",
    ]);
    expect(rows.filter((row) => row.kind === "remove" || row.kind === "add").map((row) => row.text)).toEqual([
      "line 50",
      "changed 50",
      "line 250",
      "changed 250",
    ]);
  });

  it("resolves first tool path from location, diff, input path, then undefined", () => {
    expect(firstToolPath(details({ locations: [{ path: "/workspace/located.ts", line: 4 }] }))).toEqual({
      path: "/workspace/located.ts",
      line: 4,
    });
    expect(firstToolPath(details({ content: [{ kind: "diff", path: "/workspace/diff.ts", new_text: "new" }] }))).toEqual({
      path: "/workspace/diff.ts",
      line: undefined,
    });
    expect(firstToolPath(details({ input: input({ path: "/workspace/input.ts" }) }))).toEqual({
      path: "/workspace/input.ts",
      line: undefined,
    });
    expect(firstToolPath(details())).toBeUndefined();
  });
});

function details(overrides: Partial<ActivityToolDetails> = {}): ActivityToolDetails {
  return {
    locations: [],
    content: [],
    ...overrides,
  };
}

function input(overrides: Partial<NonNullable<ActivityToolDetails["input"]>> = {}): NonNullable<ActivityToolDetails["input"]> {
  return {
    command: [],
    fields: [],
    ...overrides,
  };
}

function output(overrides: Partial<NonNullable<ActivityToolDetails["output"]>> = {}): NonNullable<ActivityToolDetails["output"]> {
  return {
    fields: [],
    ...overrides,
  };
}

function toolStep(overrides: Partial<Extract<ActivityStep, { kind: "tool" }>> = {}): Extract<ActivityStep, { kind: "tool" }> {
  return {
    kind: "tool",
    name: "search",
    status: "completed",
    ...overrides,
  };
}
