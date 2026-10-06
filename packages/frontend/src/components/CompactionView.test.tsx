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
  it("shows a stale in-progress row as a static rule without an expander", () => {
    const tree = render({ status: "in_progress" });

    expect(JSON.stringify(tree.toJSON())).toContain("Compacting context…");
    expect(tree.root.findByProps({ className: "compaction-row" }).props["data-live"]).toBe(false);
    expect(tree.root.findAllByType("button")).toHaveLength(0);
    expect(tree.root.findAllByType("time")).toHaveLength(0);
  });

  it("becomes the live indicator with elapsed time while the turn runs", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-01T00:01:30Z"));
    try {
      const tree = render({ live: true, liveStartedAt: "2026-01-01T00:00:00Z", status: "in_progress" });

      expect(tree.root.findByProps({ className: "compaction-row" }).props["data-live"]).toBe(true);
      expect(tree.root.findByProps({ role: "status" }).props["aria-live"]).toBe("polite");
      expect(tree.root.findByType("time").props).toMatchObject({ children: "1:30", dateTime: "PT90S" });
      // A partial summary must not make the live indicator a disclosure.
      const streaming = render({ live: true, status: "in_progress", summary: "Goal:" });
      expect(streaming.root.findAllByType("button")).toHaveLength(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it("treats a finished row as static even while the turn keeps running", () => {
    const tree = render({ live: true, liveStartedAt: "2026-01-01T00:00:00Z", status: "completed" });

    expect(tree.root.findByProps({ className: "compaction-row" }).props["data-live"]).toBe(false);
    expect(tree.root.findAllByType("time")).toHaveLength(0);
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

    // The frame offers a second way out below a long summary.
    act(() => tree.root.findByProps({ className: "compaction-collapse" }).props.onClick());

    expect(tree.root.findByProps({ "aria-label": "Expand context summary" }).props["aria-expanded"]).toBe(false);
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
