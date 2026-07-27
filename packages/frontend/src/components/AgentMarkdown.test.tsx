import { renderToStaticMarkup } from "react-dom/server";
import { act, create } from "react-test-renderer";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AgentMarkdown } from "./AgentMarkdown";

describe("AgentMarkdown", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders GFM markdown for agent messages", () => {
    const html = renderToStaticMarkup(
      <AgentMarkdown text={"Yes: **example.com**\n\n- Use `.com`\n- Redirect `.net`\n\n| registrar | price |\n| - | - |\n| Cloudflare | $10 |"} />,
    );

    expect(html).toContain("<strong>example.com</strong>");
    expect(html).toContain("<ul>");
    expect(html).toContain("<code>.com</code>");
    expect(html).toContain("<table>");
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

  it("adds an accessible Copy action to fenced code blocks but not inline code", () => {
    const html = renderToStaticMarkup(
      <AgentMarkdown text={"Use `inline` first.\n\n```ts\nconst ready = true;\n```"} />,
    );

    expect(html).toContain('aria-label="Copy code"');
    expect(html).toContain('class="agent-markdown-code-block"');
    expect(html.match(/aria-label="Copy code"/g)).toHaveLength(1);
  });

  it("copies each fenced block independently without Markdown fences or language markers", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AgentMarkdown text={"```ts\nconst first = true;\n```\n\n```py\ndef second():\n    return 2\n```"} />,
      );
    });
    const buttons = tree!.root.findAllByProps({ "aria-label": "Copy code" });

    expect(buttons).toHaveLength(2);
    await act(async () => {
      await buttons[1]!.props.onClick();
    });

    expect(writeText).toHaveBeenCalledOnce();
    expect(writeText).toHaveBeenCalledWith("def second():\n    return 2");
    expect(tree!.root.findAllByProps({ "aria-label": "Copy code" })).toHaveLength(1);
    expect(tree!.root.findByProps({ "aria-label": "Code copied" }).props.title).toBe("Copied");
  });

  it("reports clipboard failure without claiming success", async () => {
    vi.stubGlobal("navigator", { clipboard: { writeText: vi.fn().mockRejectedValue(new Error("denied")) } });
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(<AgentMarkdown text={"```\ncopy me\n```"} />);
    });

    await act(async () => {
      await tree!.root.findByProps({ "aria-label": "Copy code" }).props.onClick();
    });

    expect(tree!.root.findAllByProps({ "aria-label": "Code copied" })).toHaveLength(0);
    expect(tree!.root.findByProps({ "aria-label": "Copy code failed" }).props.title).toBe("Copy failed");
  });
});
