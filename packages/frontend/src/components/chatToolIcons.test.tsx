import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { toolKindIcon } from "./chatToolIcons";

describe("chat tool icons", () => {
  it("uses a distinct icon for every defined ACP tool kind", () => {
    const icons = ["read", "edit", "delete", "move", "search", "execute", "think", "fetch", "switch_mode"]
      .map((kind) => renderToStaticMarkup(toolKindIcon(kind, 12)).match(/lucide-([a-z0-9-]+)/)?.[1]);

    expect(icons.every(Boolean)).toBe(true);
    expect(new Set(icons).size).toBe(icons.length);
    expect(renderToStaticMarkup(toolKindIcon("skill", 12))).toContain("lucide-book-open");
    expect(renderToStaticMarkup(toolKindIcon("other", 12))).toContain("lucide-wrench");
  });
});
