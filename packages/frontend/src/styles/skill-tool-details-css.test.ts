import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./app/skill-tool-details.css", import.meta.url), "utf8");

describe("expanded skill document styles", () => {
  it("uses the chat scroll surface instead of nesting another scroll region", () => {
    const markdownRule = css.match(/\.skill-tool-markdown\.chat-agent\s*{([^}]*)}/)?.[1] ?? "";

    expect(markdownRule).not.toMatch(/max-height\s*:/);
    expect(markdownRule).not.toMatch(/overflow\s*:/);
    expect(markdownRule).not.toMatch(/border-/);
  });

  it("gives skill instruction headings a visible hierarchy", () => {
    expect(css).toMatch(/\.skill-tool-markdown h1\s*{[^}]*font-size:\s*16px;[^}]*font-weight:\s*600;/);
    expect(css).toMatch(/\.skill-tool-markdown h2\s*{[^}]*font-size:\s*14px;[^}]*font-weight:\s*600;/);
    expect(css).toMatch(/\.skill-tool-markdown h3\s*{[^}]*font-size:\s*13px;[^}]*font-weight:\s*600;/);
  });
});
