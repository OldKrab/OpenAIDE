// @vitest-environment jsdom

import { act, create } from "react-test-renderer";
import { describe, expect, it } from "vitest";
import type { AgentPlan } from "@openaide/app-shell-contracts";
import { AgentPlanView, CompletedPlanView } from "./AgentPlan";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const plan: AgentPlan = {
  entries: [
    { content: "Inspect projection", priority: "high", status: "completed" },
    { content: "Persist replacement", priority: "medium", status: "in_progress" },
    { content: "Render plan", priority: "low", status: "pending" },
  ],
};

describe("Agent Plan", () => {
  it("opens a live plan initially and keeps the current step visible when collapsed", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<AgentPlanView plan={plan} taskId="task-open" taskStatus="active" />);
    });

    const disclosure = tree.root.findByProps({ "aria-label": "Collapse Agent Plan" });
    expect(disclosure.props["aria-expanded"]).toBe(true);
    expect(JSON.stringify(tree.toJSON())).toContain("Render plan");

    act(() => disclosure.props.onClick());

    const collapsed = JSON.stringify(tree.toJSON());
    expect(collapsed).toContain("Persist replacement");
    expect(collapsed).not.toContain("Render plan");
  });

  it("animates the current marker only while the Task is actively running", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<AgentPlanView plan={plan} taskId="task-motion" taskStatus="active" />);
    });
    expect(tree.root.findByProps({
      className: "agent-plan-status",
      "data-status": "in_progress",
    }).props["data-animated"]).toBe(true);

    act(() => {
      tree.update(<AgentPlanView plan={plan} taskId="task-motion" taskStatus="waiting" />);
    });
    expect(tree.root.findByProps({
      className: "agent-plan-status",
      "data-status": "in_progress",
    }).props["data-animated"]).toBe(false);
  });

  it("keeps a completed Plan Chat row collapsed by default", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<CompletedPlanView entries={plan.entries.map((entry) => ({ ...entry, status: "completed" }))} />);
    });

    const disclosure = tree.root.findByProps({ "aria-label": "Expand completed Plan" });
    expect(disclosure.props["aria-expanded"]).toBe(false);
    expect(JSON.stringify(tree.toJSON())).not.toContain("Inspect projection");
  });
});
