import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { agentCatalogEntry, normalizedAgentIcon } from "@openaide/app-shell-contracts";
import { AgentIcon } from "./AgentIcon";
import { ClaudeIcon } from "./ClaudeIcon";

describe("Claude brand icon", () => {
  it("resolves the built-in catalog icon without falling back to a generic icon", () => {
    expect(agentCatalogEntry("claude-code")?.icon).toBe("claude");
    expect(normalizedAgentIcon("claude")).toBe("claude");
  });

  it.each([
    { icon: "claude" as const },
    { agentId: "claude-code", icon: "sparkles" as const },
    { agentName: "Claude Code" },
  ])("renders the same brand mark for catalog, existing tasks, and name-only surfaces: %j", (props) => {
    expect(renderToStaticMarkup(<AgentIcon {...props} size={16} />))
      .toBe(renderToStaticMarkup(<ClaudeIcon size={16} />));
  });

  it("renders monochrome so it follows the surrounding text color", () => {
    const markup = renderToStaticMarkup(<ClaudeIcon size={16} />);
    expect(markup).toContain('fill="currentColor"');
  });

  it("preserves the generic sparkles icon for custom agents", () => {
    expect(renderToStaticMarkup(<AgentIcon agentId="custom" icon="sparkles" size={16} />))
      .not.toBe(renderToStaticMarkup(<ClaudeIcon size={16} />));
  });
});
