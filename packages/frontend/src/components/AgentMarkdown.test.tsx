import { renderToStaticMarkup } from "react-dom/server";
import { act, create } from "react-test-renderer";
import { afterEach, describe, expect, it, vi } from "vitest";

const postHostMessage = vi.hoisted(() => vi.fn());

vi.mock("../services/hostBridge", () => ({ postHostMessage }));

import * as shell from "../services/frontendShell";

import { AgentMarkdown } from "./AgentMarkdown";
import { AgentFileOpenContext } from "./agentFileOpen";

describe("AgentMarkdown", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    postHostMessage.mockClear();
    vi.unstubAllGlobals();
  });

  it("opens local image links and Markdown images through the File Viewer", () => {
    const openFile = vi.fn();
    for (const [reference, path] of [
      ["sandbox:/mnt/data/meal.png", "/mnt/data/meal.png"],
      ["file:///tmp/meal%20details.png", "/tmp/meal details.png"],
      ["/tmp/meal.png", "/tmp/meal.png"],
      ["screenshots/meal.png", "screenshots/meal.png"],
    ]) {
      for (const marker of ["", "!"]) {
        let tree: ReturnType<typeof create>;
        act(() => { tree = create(
          <AgentFileOpenContext.Provider value={openFile}>
            <AgentMarkdown text={`${marker}[Meal details](${reference})`} />
          </AgentFileOpenContext.Provider>,
        ); });
        const link = tree!.root.findByType("a");
        expect(tree!.root.findAllByType("img")).toHaveLength(0);
        act(() => { link.props.onClick({ preventDefault: vi.fn() }); });
        expect(openFile).toHaveBeenLastCalledWith(path, undefined);
        act(() => { tree!.unmount(); });
      }
    }
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

  it("opens explicit Web file links of any type without changing other shells", () => {
    const openFile = vi.fn();
    const currentShell = vi.spyOn(shell, "currentFrontendShell").mockReturnValue(undefined);
    const text = "[package](dist/build.vsix) [report](<reports/annual report.pdf>) [license](LICENSE) [web](https://example.com)";
    const renderMarkdown = () => <AgentFileOpenContext.Provider value={openFile}><AgentMarkdown text={text} /></AgentFileOpenContext.Provider>;
    let tree: ReturnType<typeof create>;
    act(() => { tree = create(renderMarkdown()); });
    expect(tree!.root.findAllByType("a").every((link) => !link.props.onClick)).toBe(true);
    act(() => { tree!.unmount(); });
    currentShell.mockReturnValue({ fileViewerDownloads: { save: vi.fn() } } as unknown as shell.FrontendShell);
    act(() => { tree = create(renderMarkdown()); });
    const links = tree!.root.findAllByType("a");
    act(() => { for (const link of links.slice(0, 3)) link.props.onClick({ preventDefault: vi.fn() }); });
    expect(openFile.mock.calls).toEqual([["dist/build.vsix", undefined], ["reports/annual report.pdf", undefined], ["LICENSE", undefined]]);
    expect(links[3].props.onClick).toBeUndefined();
  });

  it("opens an absolute filesystem link at its line through the App Shell", () => {
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AgentMarkdown
          text={
            "[traits.rs](/home/remote-user/src/company-agent-kernel/crates/sdk-api/src/traits.rs:305)"
          }
        />,
      );
    });
    const link = tree!.root.findByType("a");
    const preventDefault = vi.fn();

    act(() => {
      link.props.onClick({ preventDefault });
    });

    expect(preventDefault).toHaveBeenCalledOnce();
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "tool.openPath",
      payload: {
        line: 305,
        path: "/home/remote-user/src/company-agent-kernel/crates/sdk-api/src/traits.rs",
      },
    });
  });

  it("preserves Windows filesystem links for App Shell opening", () => {
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(<AgentMarkdown text={"[main.ts](C:/Users/example/project/src/main.ts:42)"} />);
    });
    const link = tree!.root.findByType("a");

    act(() => {
      link.props.onClick({ preventDefault: vi.fn() });
    });

    expect(postHostMessage).toHaveBeenCalledWith({
      type: "tool.openPath",
      payload: { line: 42, path: "C:/Users/example/project/src/main.ts" },
    });
  });

  it("opens path-like inline code as an Agent File Reference", () => {
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(<AgentMarkdown text={"Start with `deploy/local-web.sh` then run it."} />);
    });
    const code = tree!.root.findByProps({ role: "link" });

    act(() => {
      code.props.onClick({ preventDefault: vi.fn() });
    });

    expect(postHostMessage).toHaveBeenCalledWith({
      type: "tool.openPath",
      payload: { path: "deploy/local-web.sh", line: undefined },
    });
  });

  it("leaves dotted identifiers that are not file paths as ordinary inline code", () => {
    const html = renderToStaticMarkup(<AgentMarkdown text={"Use `.com` domains."} />);
    expect(html).toContain("<code>.com</code>");
    expect(html).not.toContain("agent-file-ref");
  });

  it("does not turn protocol method names into Agent File References", () => {
    const html = renderToStaticMarkup(
      <AgentMarkdown text={"Call `fileViewer/open` or `tool.openPath` after the user clicks."} />,
    );
    expect(html).toContain("<code>fileViewer/open</code>");
    expect(html).toContain("<code>tool.openPath</code>");
    expect(html).not.toContain("agent-file-ref");
  });

  it("opens a File Viewer path through Chat context instead of the App Shell", () => {
    const openFile = vi.fn();
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AgentFileOpenContext.Provider value={openFile}>
          <AgentMarkdown text={"See `tool_details.rs:134`."} />
        </AgentFileOpenContext.Provider>,
      );
    });

    act(() => {
      tree!.root.findByProps({ role: "link" }).props.onClick({ preventDefault: vi.fn() });
    });

    expect(openFile).toHaveBeenCalledWith("tool_details.rs", 134);
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("resolves relative File Tab markdown hrefs without sending a raw path", () => {
    const onOpenRelativeHref = vi.fn();
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AgentMarkdown onOpenRelativeHref={onOpenRelativeHref} text={"See [notes](../notes.md#L12)."} />,
      );
    });

    act(() => {
      tree!.root.findByType("a").props.onClick({ preventDefault: vi.fn() });
    });

    expect(onOpenRelativeHref).toHaveBeenCalledWith("../notes.md#L12");
    expect(postHostMessage).not.toHaveBeenCalled();
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
    expect(html).toContain("agent-markdown-code-block");
    expect(html.match(/aria-label="Copy code"/g)).toHaveLength(1);
  });

  it("recognizes explicit Mermaid fences only when Agent Chat enables diagrams", () => {
    const enabled = renderToStaticMarkup(
      <AgentMarkdown renderDiagrams text={"```mermaid\nflowchart LR\n  A --> B\n```"} />,
    );
    const disabled = renderToStaticMarkup(
      <AgentMarkdown text={"```mermaid\nflowchart LR\n  A --> B\n```"} />,
    );
    const streaming = renderToStaticMarkup(
      <AgentMarkdown renderDiagrams streaming text={"```mermaid\nflowchart LR\n  A --> B\n```"} />,
    );

    expect(enabled).toContain("agent-mermaid");
    expect(disabled).not.toContain("agent-mermaid");
    expect(streaming).not.toContain("agent-mermaid");
    expect(streaming).toContain("language-mermaid");
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

  it("copies each quote independently while preserving its inner Markdown", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AgentMarkdown text={"> First **quote**.\n\n> **Second quote:**\n> 1. First step\n> 2. Second step\n>\n> Second paragraph."} />,
      );
    });
    const buttons = tree!.root.findAllByProps({ "aria-label": "Copy quote" });

    expect(buttons).toHaveLength(2);
    await act(async () => {
      await buttons[1]!.props.onClick();
    });

    expect(writeText).toHaveBeenCalledOnce();
    expect(writeText).toHaveBeenCalledWith(
      "**Second quote:**\n1. First step\n2. Second step\n\nSecond paragraph.",
    );
    expect(tree!.root.findAllByProps({ "aria-label": "Copy quote" })).toHaveLength(1);
    expect(tree!.root.findByProps({ "aria-label": "Quote copied" }).props.title).toBe("Copied");
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
