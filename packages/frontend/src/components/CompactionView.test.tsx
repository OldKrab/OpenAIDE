// @vitest-environment jsdom

import { act, create } from "react-test-renderer";
import { describe, expect, it, vi } from "vitest";

vi.mock("./AgentMarkdown", () => ({
  AgentMarkdown: ({ text }: { text: string }) => <div data-testid="summary">{text}</div>,
}));

import { CompactionView } from "./CompactionView";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function render(props: Parameters<typeof CompactionView>[0]) {
  let tree!: ReturnType<typeof create>;
  act(() => {
    tree = create(<CompactionView {...props} />);
  });
  return tree;
}

describe("CompactionView", () => {
  it("shows progress without an expander until a summary exists", () => {
    const tree = render({ status: "in_progress" });

    expect(JSON.stringify(tree.toJSON())).toContain("Compacting context…");
    expect(tree.root.findAllByType("button")).toHaveLength(0);
  });

  it("keeps a completed summary collapsed until the user expands it", () => {
    const tree = render({ status: "completed", summary: "Goal: ship compaction" });

    expect(JSON.stringify(tree.toJSON())).toContain("Context compacted");
    const toggle = tree.root.findByProps({ "aria-label": "Expand context summary" });
    expect(toggle.props["aria-expanded"]).toBe(false);
    expect(tree.root.findByProps({ className: "compaction-disclosure" }).props).toMatchObject({
      "aria-hidden": true,
      "data-open": false,
      inert: true,
    });

    act(() => toggle.props.onClick());

    expect(tree.root.findByProps({ "aria-label": "Collapse context summary" }).props["aria-expanded"]).toBe(true);
    expect(tree.root.findByProps({ className: "compaction-disclosure" }).props["data-open"]).toBe(true);
  });

  it("surfaces the failure reason", () => {
    const tree = render({ status: "failed", error: "context window exceeded" });

    const text = JSON.stringify(tree.toJSON());
    expect(text).toContain("Context compaction failed");
    expect(text).toContain("context window exceeded");
  });

  it("labels cancelled and unrecognized lifecycles without inventing completion", () => {
    expect(JSON.stringify(render({ status: "cancelled" }).toJSON())).toContain("Context compaction cancelled");
    expect(JSON.stringify(render({ status: "unknown" }).toJSON())).not.toContain("Context compacted");
  });
});
