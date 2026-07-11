import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { toolKindIcon } from "./chatToolIcons";

describe("chat tool icons", () => {
  it("uses a distinct open-book icon for activated skills", () => {
    expect(renderToStaticMarkup(toolKindIcon("skill", 12))).toContain("lucide-book-open");
  });
});
