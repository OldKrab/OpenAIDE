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

  it("escapes slash command labels and titles", () => {
    const commandCatalog: AgentCommandsCatalog = {
      agent_id: "codex",
      commands: [{ name: "ship", description: "Use <fast> & safe mode", input_hint: "\"note\"" }],
      status: "ready",
    };

    expect(renderEditorHtml("/ship ", commandCatalog)).toBe(
      "<span class=\"composer-command-token\" title=\"/ship: Use &lt;fast&gt; &amp; safe mode Argument: &quot;note&quot;.\">/ship</span> ",
    );
  });

  it("renders workspace file mentions as syntax-only tokens", () => {
    expect(renderEditorHtml('Read @src/main.rs and @"docs/team deck.pptx"', undefined)).toBe(
      'Read <span class="composer-file-token" title="Workspace file">@src/main.rs</span> and <span class="composer-file-token" title="Workspace file">@&quot;docs/team deck.pptx&quot;</span>',
    );
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
