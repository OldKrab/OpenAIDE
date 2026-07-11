import { describe, expect, it } from "vitest";
import { parseSkillDocument } from "./skillToolViewModel";

describe("skill tool view model", () => {
  it("extracts frontmatter metadata and returns only the Markdown body", () => {
    expect(
      parseSkillDocument('---\nname: tdd\ndescription: "Test-first workflow"\n---\n\n# Test-Driven Development\n\nUse tests.'),
    ).toEqual({
      name: "tdd",
      description: "Test-first workflow",
      body: "# Test-Driven Development\n\nUse tests.",
    });
  });

  it("supports CRLF frontmatter and leaves documents without frontmatter intact", () => {
    expect(parseSkillDocument("---\r\nname: tdd\r\ndescription: Test first\r\n---\r\n\r\n# Workflow").body).toBe("# Workflow");
    expect(parseSkillDocument("# Ordinary Markdown\n\nNo metadata.")).toEqual({
      body: "# Ordinary Markdown\n\nNo metadata.",
    });
  });

  it("does not discard malformed frontmatter without a closing delimiter", () => {
    const content = "---\nname: tdd\n# Still content";
    expect(parseSkillDocument(content)).toEqual({ body: content });
  });
});
