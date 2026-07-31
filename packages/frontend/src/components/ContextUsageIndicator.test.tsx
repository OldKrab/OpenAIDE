// @vitest-environment jsdom

import { act, create } from "react-test-renderer";
import { describe, expect, it } from "vitest";
import type { ConfigOptionsCatalog } from "@openaide/app-shell-contracts";
import { ComposerWithContextUsage } from "./ContextUsageIndicator";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("Composer context usage", () => {
  it("keeps numbers hidden until the edge meter opens the detached details panel", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <ComposerWithContextUsage
          configOptions={modelOptions}
          usage={{
            used_tokens: 31_000,
            capacity_tokens: 258_400,
            cost: { amount: "0.42", currency: "USD" },
            last_turn: {
              total_tokens: 168_500,
              input_tokens: 1_700,
              output_tokens: 118,
              reasoning_tokens: 14,
              cached_read_tokens: 166_700,
              cached_write_tokens: 86,
            },
          }}
        >
          <section className="composer" />
        </ComposerWithContextUsage>,
      );
    });

    const meter = tree.root.findByProps({
      "aria-label": "Context usage: 12% used. Show details",
    });
    expect(meter.props["aria-expanded"]).toBe(false);
    expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(0);

    act(() => meter.props.onClick());

    expect(meter.props["aria-expanded"]).toBe(true);
    const panel = tree.root.findByProps({ role: "dialog" });
    expect(panel.props["data-placement"]).toBe("anchor");
    expect(textContent(panel)).toContain("12% used");
    expect(textContent(panel)).toContain("GPT-5.6-Sol");
    expect(textContent(panel)).toContain("Cache read");
    expect(textContent(panel)).toContain("Cache write");
    expect(textContent(panel)).toContain("Session cost");
  });

  it("keeps details closed on hover and anchors the tooltip to the filled percentage", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <ComposerWithContextUsage
          usage={{
            used_tokens: 50_000,
            capacity_tokens: 100_000,
          }}
        >
          <section className="composer" />
        </ComposerWithContextUsage>,
      );
    });

    const meter = tree.root.findByProps({
      "aria-label": "Context usage: 50% used. Show details",
    });
    expect(meter.props.onPointerEnter).toBeUndefined();
    expect(meter.props.onPointerMove).toBeUndefined();

    expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(0);
    const tooltip = tree.root.findByProps({ role: "tooltip" });
    expect(tooltip.props.style).toMatchObject({
      "--context-usage-percent": "50%",
    });
  });
});

const modelOptions: ConfigOptionsCatalog = {
  agent_id: "codex",
  status: "ready",
  options: [{
    id: "model",
    label: "Model",
    category: "model",
    kind: "select",
    current_value: { type: "id", value: "gpt-5.6-sol" },
    values: [{ id: "gpt-5.6-sol", label: "GPT-5.6-Sol" }],
  }],
};

function textContent(node: ReturnType<ReturnType<typeof create>["root"]["findByProps"]>) {
  return node.findAll(() => true)
    .flatMap((child) => child.children)
    .filter((child): child is string => typeof child === "string")
    .join("");
}
