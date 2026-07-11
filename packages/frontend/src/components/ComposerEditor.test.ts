import { describe, expect, it, vi } from "vitest";
import type { AgentCommandsCatalog } from "@openaide/app-shell-contracts";
import { renderEditorHtml, restoreEditorSelection } from "./ComposerEditor";

describe("ComposerEditor markup", () => {
  it("renders plain text as replaceable escaped editor HTML", () => {
    expect(renderEditorHtml("one < two\nthree & four", undefined)).toBe("one &lt; two<br>three &amp; four");
    expect(renderEditorHtml("", undefined)).toBe("");
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
});
