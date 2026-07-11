import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AgentMarkdown } from "./AgentMarkdown";

describe("AgentMarkdown", () => {
  it("renders GFM markdown for agent messages", () => {
    const html = renderToStaticMarkup(
      <AgentMarkdown text={"Yes: **openaide.com**\n\n- Use `.com`\n- Redirect `.ai`\n\n| registrar | price |\n| - | - |\n| Cloudflare | $10 |"} />,
    );

    expect(html).toContain("<strong>openaide.com</strong>");
    expect(html).toContain("<ul>");
    expect(html).toContain("<code>.com</code>");
    expect(html).toContain("<table>");
  });

  it("places the streaming caret inside the final rendered Markdown block", () => {
    const cases = [
      { text: "Streaming words", ending: 'Streaming words<span aria-hidden="true" class="chat-streaming-caret"></span></p>' },
      { text: "- first\n- last", ending: 'last<span aria-hidden="true" class="chat-streaming-caret"></span></li>' },
      {
        text: "| item | price |\n| --- | --- |\n| plan | $10 |",
        ending: '$10<span aria-hidden="true" class="chat-streaming-caret"></span></td>',
      },
      {
        text: "```text\nlast line\n```",
        ending: 'last line\n</code><span aria-hidden="true" class="chat-streaming-caret"></span></pre>',
      },
    ];

    for (const item of cases) {
      expect(renderToStaticMarkup(<AgentMarkdown streaming text={item.text} />)).toContain(item.ending);
    }
  });

  it("drops raw HTML from agent messages", () => {
    const html = renderToStaticMarkup(<AgentMarkdown text={"<script>alert(1)</script>\n\n<strong>raw</strong>"} />);

    expect(html).not.toContain("<script");
    expect(html).not.toContain("<strong");
    expect(html).toContain("<p>raw</p>");
  });

  it("renders links with safe browser attributes", () => {
    const html = renderToStaticMarkup(<AgentMarkdown text={"[Cloudflare](https://www.cloudflare.com/)"} />);

    expect(html).toContain('href="https://www.cloudflare.com/"');
    expect(html).toContain('rel="noreferrer"');
    expect(html).toContain('target="_blank"');
  });

  it("does not render unsafe javascript links", () => {
    const html = renderToStaticMarkup(<AgentMarkdown text={"[bad](javascript:alert(1))"} />);

    expect(html).not.toContain("href=");
    expect(html).toContain("<span>bad</span>");
  });

  it("renders data image links as compact previews instead of visible base64 markdown", () => {
    const payload = "aW1hZ2U=".repeat(600);
    const html = renderToStaticMarkup(
      <AgentMarkdown text={`Here is it:\n\n[@image](data:image/png;base64,${payload})`} />,
    );

    expect(html).toContain('class="agent-markdown-image-link"');
    expect(html).toContain(`src="data:image/png;base64,${payload}"`);
    expect(html).toContain('alt="@image"');
    expect(html).not.toContain("[@image]");
    expect(html).not.toContain("href=\"data:image/png;base64,");
  });
});
