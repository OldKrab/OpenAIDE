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

  it("offers Stop only for a running Subagent the Agent can stop", () => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    const onStop = vi.fn(async () => true);
    const stoppable = { ...entry(undefined), capabilities: { cancel: true, close: false } };
    const cases: Array<[SubagentCatalogEntrySnapshot, boolean]> = [
      [stoppable, true],
      [{ ...stoppable, status: "waitingForActivity" }, true],
      [{ ...stoppable, status: "completed" }, false],
      [entry(undefined), false],
    ];
    for (const [selected, offered] of cases) {
      let view: ReturnType<typeof create>;
      act(() => {
        view = create(<SubagentInspectionBar onReturn={vi.fn()} onStop={onStop} selected={selected} />);
      });
      expect(view!.root.findAllByProps({ className: "subagent-inspection-stop" })).toHaveLength(offered ? 1 : 0);
    }
    let withoutCallback: ReturnType<typeof create>;
    act(() => {
      withoutCallback = create(<SubagentInspectionBar onReturn={vi.fn()} selected={stoppable} />);
    });
    expect(withoutCallback!.root.findAllByProps({ className: "subagent-inspection-stop" })).toHaveLength(0);
  });

  it("stays pending after an accepted stop until the reported status changes", async () => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    const onStop = vi.fn(async () => true);
    const running = { ...entry(undefined), capabilities: { cancel: true, close: false } };
    let view: ReturnType<typeof create>;
    act(() => {
      view = create(<SubagentInspectionBar onReturn={vi.fn()} onStop={onStop} selected={running} />);
    });
    const stop = () => view!.root.findByProps({ className: "subagent-inspection-stop" });

    await act(async () => stop().props.onClick());
    expect(onStop).toHaveBeenCalledWith(running.subagentId);
    expect(stop().props.disabled).toBe(true);
    expect(stop().children.join("")).toBe("Stopping…");

    act(() => {
      view!.update(<SubagentInspectionBar onReturn={vi.fn()} onStop={onStop} selected={{ ...running, status: "cancelled" }} />);
    });
    expect(view!.root.findAllByProps({ className: "subagent-inspection-stop" })).toHaveLength(0);
  });

  it("re-enables Stop when App Server rejects the request", async () => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    const onStop = vi.fn(async () => false);
    const running = { ...entry(undefined), capabilities: { cancel: true, close: false } };
    let view: ReturnType<typeof create>;
    act(() => {
      view = create(<SubagentInspectionBar onReturn={vi.fn()} onStop={onStop} selected={running} />);
    });
    const stop = () => view!.root.findByProps({ className: "subagent-inspection-stop" });

    await act(async () => stop().props.onClick());
    expect(stop().props.disabled).toBe(false);
    expect(stop().children.join("")).toBe("Stop");
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
