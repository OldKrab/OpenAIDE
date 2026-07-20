import { describe, expect, it, vi } from "vitest";
import type { AgentCommandsCatalog } from "@openaide/app-shell-contracts";
import { captureFocusedEditorSelection, renderEditorHtml, restoreEditorSelection } from "./ComposerEditor";

describe("ComposerEditor markup", () => {
  it("renders plain text as replaceable escaped editor HTML", () => {
    expect(renderEditorHtml("one < two\nthree & four", undefined)).toBe("one &lt; two<br>three &amp; four");
    expect(renderEditorHtml("", undefined)).toBe("");
  });

  it("renders a trailing newline as a visible empty line", () => {
    expect(renderEditorHtml("line one\n", undefined)).toBe("line one<br><br>");
  });

  it("renders escaped slash-command hover metadata without a native title", () => {
    const commandCatalog: AgentCommandsCatalog = {
      agent_id: "codex",
      commands: [{ name: "ship", description: "Use <fast> & safe mode", input_hint: "\"note\"" }],
      status: "ready",
    };

    const html = renderEditorHtml("/ship ", commandCatalog);

    expect(html).toBe(
      "<span class=\"composer-reference-token composer-command-token\" data-reference-description=\"Use &lt;fast&gt; &amp; safe mode\" data-reference-kind=\"command\" data-reference-label=\"/ship\" data-reference-type=\"Skill\" spellcheck=\"false\">/ship</span> ",
    );
    expect(html).not.toContain("title=");
  });

  it("renders path-derived file hover metadata without a native title", () => {
    const html = renderEditorHtml('Read @src/main.rs and @"docs/team deck.pptx"', undefined);

    expect(html).toBe(
      'Read <span class="composer-reference-token composer-file-token" data-reference-description="Rust · src" data-reference-file-kind="rust" data-reference-kind="file" data-reference-label="main.rs" data-reference-path="src/main.rs" data-reference-type="Workspace file" spellcheck="false">@src/main.rs</span> and <span class="composer-reference-token composer-file-token" data-reference-description="PowerPoint · docs" data-reference-file-kind="presentation" data-reference-kind="file" data-reference-label="team deck.pptx" data-reference-path="docs/team deck.pptx" data-reference-type="Workspace file" spellcheck="false">@&quot;docs/team deck.pptx&quot;</span>',
    );
    expect(html).not.toContain("title=");
  });

  it("restores the focused editor selection after refreshed markup is committed", () => {
    const textNode = { textContent: "keep cursor stable" };
    const range = {
      setEnd: vi.fn(),
      setStart: vi.fn(),
    };
    const selection = {
      addRange: vi.fn(),
      removeAllRanges: vi.fn(),
    };
    const rootOwnerDocument: {
      activeElement: unknown;
      createRange: () => typeof range;
      createTreeWalker: () => { nextNode: () => typeof textNode | null };
      defaultView: { NodeFilter: { SHOW_TEXT: number } };
      getSelection: () => typeof selection;
    } = {
      activeElement: undefined,
      createRange: () => range,
      createTreeWalker: () => {
        let returned = false;
        return {
          nextNode: () => {
            if (returned) return null;
            returned = true;
            return textNode;
          },
        };
      },
      defaultView: { NodeFilter: { SHOW_TEXT: 4 } },
      getSelection: () => selection,
    };
    const root = {
      childNodes: [textNode],
      ownerDocument: rootOwnerDocument,
    };
    root.ownerDocument.activeElement = root;

    restoreEditorSelection(root as unknown as HTMLElement, { start: 5, end: 11 });

    expect(range.setStart).toHaveBeenCalledWith(textNode, 5);
    expect(range.setEnd).toHaveBeenCalledWith(textNode, 11);
    expect(selection.removeAllRanges).toHaveBeenCalledTimes(1);
    expect(selection.addRange).toHaveBeenCalledWith(range);
  });

  it("counts rendered line breaks when capturing and restoring a multiline end caret", () => {
    const firstLine = textNode("first");
    const firstBreak = breakNode();
    const secondLine = textNode("second");
    const secondBreak = breakNode();
    const lastLine = textNode("last");
    const root = elementNode([firstLine, firstBreak, secondLine, secondBreak, lastLine]);
    const selectionRange = {
      endContainer: lastLine,
      endOffset: 4,
      startContainer: lastLine,
      startOffset: 4,
    };
    const restoredRange = {
      selectNodeContents: vi.fn(),
      setEnd: vi.fn(),
      setStart: vi.fn(),
      toString: () => "firstsecondlast",
    };
    const selection = {
      addRange: vi.fn(),
      getRangeAt: () => selectionRange,
      rangeCount: 1,
      removeAllRanges: vi.fn(),
    };
    const ownerDocument = {
      activeElement: root,
      createRange: () => restoredRange,
      defaultView: { NodeFilter: { SHOW_TEXT: 4 } },
      getSelection: () => selection,
    };
    Object.assign(root, { ownerDocument });

    expect(captureFocusedEditorSelection(root as unknown as HTMLElement)).toEqual({ start: 17, end: 17 });

    restoreEditorSelection(root as unknown as HTMLElement, { start: 17, end: 17 });

    expect(restoredRange.setStart).toHaveBeenCalledWith(lastLine, 4);
    expect(restoredRange.setEnd).toHaveBeenCalledWith(lastLine, 4);
  });
});

function textNode(textContent: string) {
  return { childNodes: [], nodeType: 3, textContent };
}

function breakNode() {
  return { childNodes: [], nodeType: 1, tagName: "BR", textContent: "" };
}

function elementNode(childNodes: unknown[]) {
  const root = { childNodes, nodeType: 1, tagName: "DIV", textContent: "firstsecondlast" };
  return Object.assign(root, {
    contains: (candidate: unknown) => candidate === root || childNodes.includes(candidate),
  });
}
