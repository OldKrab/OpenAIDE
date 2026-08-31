import { act, create } from "react-test-renderer";
import { describe, expect, it, vi } from "vitest";
import type { SubagentId } from "@openaide/app-server-client";

import { SubagentNavigator } from "./SubagentNavigator";

describe("SubagentNavigator", () => {
  it("offers direct Main Agent return and all nested histories from one control", () => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    const onSelect = vi.fn();
    let view: ReturnType<typeof create>;
    act(() => {
      view = create(<SubagentNavigator
        entries={[
          {
            subagentId: "subagent_11111111111111111111111111111111" as SubagentId,
            name: "Explorer",
            delegatedTask: "Inspect",
            status: "running",
            capabilities: { cancel: false, close: false },
            spawnedOrder: 1,
            historyRevision: 2,
          },
          {
            subagentId: "subagent_22222222222222222222222222222222" as SubagentId,
            parentSubagentId: "subagent_11111111111111111111111111111111" as SubagentId,
            name: "Reviewer",
            delegatedTask: "Review",
            status: "completed",
            capabilities: { cancel: false, close: false },
            spawnedOrder: 2,
            historyRevision: 1,
          },
        ]}
        onSelect={onSelect}
        selectedId={"subagent_22222222222222222222222222222222" as SubagentId}
        unseen={new Set(["subagent_11111111111111111111111111111111"])}
      />);
    });

    const trigger = view!.root.findByProps({
      "aria-label": "Switch agent. Currently viewing Reviewer",
    });
    act(() => trigger.props.onClick());

    const choices = view!.root.findAllByProps({ role: "menuitemradio" });
    expect(choices).toHaveLength(3);
    expect(choices[0]!.findByType("strong").children.join("")).toBe("Main Agent");
    expect(choices.flatMap((choice) => choice.findAllByType("small"))
      .map((copy) => copy.children.join(""))).not.toContain("Inspect");
    expect(choices.flatMap((choice) => choice.findAllByType("small"))
      .map((copy) => copy.children.join(""))).not.toContain("Review");

    act(() => choices[0]!.props.onClick());
    expect(onSelect).toHaveBeenCalledWith(undefined);
  });
});
