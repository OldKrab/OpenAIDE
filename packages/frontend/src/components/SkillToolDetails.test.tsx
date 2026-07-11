import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ActivityToolDetails } from "@openaide/app-shell-contracts";

describe("skill tool details", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.stubGlobal("window", { acquireVsCodeApi: undefined });
  });

  it("renders skill instructions as Markdown without exposing YAML frontmatter", async () => {
    const { ChatToolDetails } = await import("./ChatToolDetailsView");
    const details: ActivityToolDetails = {
      locations: [{ path: "/home/user/.agents/skills/tdd/SKILL.md" }],
      content: [],
      output: {
        formatted_output:
          '---\nname: tdd\ndescription: "Test-first workflow"\n---\n\n# Test-Driven Development\n\n- Write one test\n- Make it pass\n\n`npm test`\n\n<script>alert(1)</script>',
        fields: [],
      },
    };

    const html = renderToStaticMarkup(
      <ChatToolDetails
        details={details}
        step={{ kind: "tool", name: "skill", status: "completed", input_summary: "tdd" }}
      />,
    );

    expect(html).toContain("<h1>Test-Driven Development</h1>");
    expect(html).toContain("<li>Write one test</li>");
    expect(html).toContain("<code>npm test</code>");
    expect(html).not.toContain("Test-first workflow");
    expect(html).not.toContain("/home/user/.agents/skills/tdd/SKILL.md");
    expect(html).not.toContain("name: tdd");
    expect(html).not.toContain("<script");
    expect(html).not.toContain("alert(1)");
    expect(html).not.toContain("read-tool-line-number");
  });
});
