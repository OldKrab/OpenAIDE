import { act, create } from "react-test-renderer";
import { describe, expect, it, vi } from "vitest";
import type { SubagentCatalogEntrySnapshot, SubagentId } from "@openaide/app-server-client";

import { SubagentInspectionBar } from "./SubagentInspectionBar";

describe("SubagentInspectionBar", () => {
  it("summarizes Agent-reported details and discloses them with their labels", () => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    let view: ReturnType<typeof create>;
    act(() => {
      view = create(<SubagentInspectionBar
        onReturn={vi.fn()}
        selected={entry([
          { label: "Agent type", value: "reviewer" },
          { label: "Model", value: "test-model" },
        ])}
      />);
    });

    const trigger = view!.root.findByProps({
      "aria-label": "Subagent details: Agent type reviewer, Model test-model",
    });
    expect(trigger.findByProps({ className: "subagent-details-summary" }).children.join(""))
      .toBe("reviewer · test-model");
    expect(view!.root.findAllByType("dt")).toHaveLength(0);

    act(() => trigger.props.onClick());
    expect(view!.root.findAllByType("dt").map((term) => term.children.join("")))
      .toEqual(["Agent type", "Model"]);
    expect(view!.root.findAllByType("dd").map((value) => value.children.join("")))
      .toEqual(["reviewer", "test-model"]);
  });

  it("omits the disclosure when the Agent reported no details and still returns to Main Agent", () => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    const onReturn = vi.fn();
    let view: ReturnType<typeof create>;
    act(() => {
      view = create(<SubagentInspectionBar onReturn={onReturn} selected={entry(undefined)} />);
    });

    expect(view!.root.findAllByProps({ className: "subagent-details-trigger" })).toHaveLength(0);
    act(() => view!.root.findByProps({ className: "subagent-inspection-return" }).props.onClick());
    expect(onReturn).toHaveBeenCalledOnce();
  });
});

function entry(details: SubagentCatalogEntrySnapshot["details"]): SubagentCatalogEntrySnapshot {
  return {
    subagentId: "subagent_11111111111111111111111111111111" as SubagentId,
    name: "Reviewer",
    delegatedTask: "Review",
    status: "running",
    capabilities: { cancel: false, close: false },
    spawnedOrder: 1,
    historyRevision: 1,
    details,
  };
}
