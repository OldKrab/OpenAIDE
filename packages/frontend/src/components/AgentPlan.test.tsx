// @vitest-environment jsdom

import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentPlan } from "@openaide/app-shell-contracts";
import { AgentPlanView, ClosedPlanView, CompletedPlanView, resetAgentPlanDisclosure } from "./AgentPlan";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const plan: AgentPlan = {
  entries: [
    { content: "Inspect projection", priority: "high", status: "completed" },
    { content: "Persist replacement", priority: "medium", status: "in_progress" },
    { content: "Render plan", priority: "low", status: "pending" },
  ],
};

describe("Agent Plan", () => {
  beforeEach(() => {
    vi.stubGlobal("sessionStorage", memoryStorage());
  });

  it("collapses a live plan initially and keeps the current step visible", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<AgentPlanView plan={plan} taskId="task-open" taskStatus="active" />);
    });

    const disclosure = tree.root.findByProps({ "aria-label": "Expand Agent Plan" });
    expect(disclosure.props["aria-expanded"]).toBe(false);
    expect(JSON.stringify(tree.toJSON())).toContain("Persist replacement");
    expect(tree.root.findByProps({ className: "agent-plan-disclosure" }).props).toMatchObject({
      "aria-hidden": true,
      "data-open": false,
      inert: true,
    });

    act(() => disclosure.props.onClick());

    expect(tree.root.findByProps({ className: "agent-plan-disclosure" }).props).toMatchObject({
      "aria-hidden": false,
      "data-open": true,
      inert: undefined,
    });
  });

  it("restores a live plan's disclosure state after remounting", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<AgentPlanView plan={plan} taskId="task-reload" taskStatus="active" />);
    });
    act(() => tree.root.findByProps({ "aria-label": "Expand Agent Plan" }).props.onClick());
    expect(sessionStorage.getItem("openaide.agentPlanDisclosure:task-reload")).toBe("expanded");
    act(() => tree.unmount());

    act(() => {
      tree = create(<AgentPlanView plan={plan} taskId="task-reload" taskStatus="active" />);
    });

    expect(tree.root.findByProps({ "aria-label": "Collapse Agent Plan" }).props["aria-expanded"]).toBe(true);
  });

  it("clears retained disclosure when the current plan ends", () => {
    sessionStorage.setItem("openaide.agentPlanDisclosure:task-ended", "expanded");

    resetAgentPlanDisclosure("task-ended");

    expect(sessionStorage.getItem("openaide.agentPlanDisclosure:task-ended")).toBeNull();
  });

  it("animates the current marker only while the Task is actively running", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<AgentPlanView plan={plan} taskId="task-motion" taskStatus="active" />);
    });
    expect(tree.root.findAllByProps({
      className: "agent-plan-status",
      "data-animated": true,
      "data-status": "in_progress",
    })).toHaveLength(1);

    act(() => {
      tree.update(<AgentPlanView plan={plan} taskId="task-motion" taskStatus="waiting" />);
    });
    expect(tree.root.findAllByProps({
      className: "agent-plan-status",
      "data-animated": true,
      "data-status": "in_progress",
    })).toHaveLength(0);
  });

  it("lets the user close the current Plan", () => {
    const onClose = vi.fn(() => new Promise<void>(() => undefined));
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AgentPlanView
          onClose={onClose}
          plan={plan}
          taskId="task-close"
          taskStatus="active"
        />,
      );
    });

    act(() => tree.root.findByProps({ "aria-label": "Close Plan" }).props.onClick());

    expect(onClose).toHaveBeenCalledOnce();
  });

  it("keeps a completed Plan Chat row collapsed by default", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<CompletedPlanView entries={plan.entries.map((entry) => ({ ...entry, status: "completed" }))} />);
    });

    const disclosure = tree.root.findByProps({ "aria-label": "Expand completed Plan" });
    expect(disclosure.props["aria-expanded"]).toBe(false);
    expect(tree.root.findByProps({ className: "agent-plan-disclosure" }).props).toMatchObject({
      "aria-hidden": true,
      "data-open": false,
      inert: true,
    });
  });

  it("labels a user-closed Plan distinctly from completion", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<ClosedPlanView entries={plan.entries} />);
    });

    expect(JSON.stringify(tree.toJSON())).toContain("Plan closed by user");
    expect(tree.root.findByProps({ "aria-label": "Expand Plan closed by user" }).props["aria-expanded"]).toBe(false);
  });
});

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => { values.delete(key); },
    setItem: (key, value) => { values.set(key, value); },
  };
}
