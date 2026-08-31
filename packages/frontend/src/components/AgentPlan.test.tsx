// @vitest-environment jsdom

import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentPlan } from "@openaide/app-shell-contracts";
import type { ReactNode } from "react";

vi.mock("./Popup", () => ({
  PopupMenu: ({
    children,
    onOpenChange,
    open,
    trigger,
  }: {
    children: ReactNode;
    onOpenChange: (open: boolean) => void;
    open: boolean;
    trigger: (props: { onClick: () => void }) => ReactNode;
  }) => (
    <>
      {trigger({ onClick: () => onOpenChange(!open) })}
      {open ? children : null}
    </>
  ),
}));

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

  it("uses an expanded default only when the user has no retained preference", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AgentPlanView
          defaultOpen
          plan={plan}
          taskId="task-default-expanded"
          taskStatus="active"
        />,
      );
    });
    expect(tree.root.findByProps({ "aria-label": "Collapse Agent Plan" }).props["aria-expanded"]).toBe(true);
    act(() => tree.unmount());

    sessionStorage.setItem("openaide.agentPlanDisclosure:task-retained-collapse", "collapsed");
    act(() => {
      tree = create(
        <AgentPlanView
          defaultOpen
          plan={plan}
          taskId="task-retained-collapse"
          taskStatus="active"
        />,
      );
    });
    expect(tree.root.findByProps({ "aria-label": "Expand Agent Plan" }).props["aria-expanded"]).toBe(false);
  });

  it("shows a Plan panel without a redundant disclosure control", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AgentPlanView
          collapsible={false}
          plan={plan}
          taskId="task-panel"
          taskStatus="active"
        />,
      );
    });

    expect(tree.root.findAllByProps({ "aria-label": "Expand Agent Plan" })).toHaveLength(0);
    expect(tree.root.findAllByProps({ "aria-label": "Collapse Agent Plan" })).toHaveLength(0);
    expect(tree.root.findByProps({ className: "agent-plan-heading" }).type).toBe("div");
    expect(tree.root.findByProps({ className: "agent-plan-disclosure" }).props).toMatchObject({
      "aria-hidden": false,
      "data-open": true,
      inert: undefined,
    });
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

  it("hides a Plan drawer without dismissing it", () => {
    const onHide = vi.fn();
    const onDismiss = vi.fn();
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AgentPlanView
          onDismiss={onDismiss}
          onHide={onHide}
          plan={plan}
          taskId="task-hide"
          taskStatus="active"
        />,
      );
    });

    act(() => tree.root.findByProps({ "data-plan-action": "hide" }).props.onClick());

    expect(onHide).toHaveBeenCalledOnce();
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it("lets the user close the current Plan from the overflow menu", () => {
    const onDismiss = vi.fn(() => new Promise<void>(() => undefined));
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AgentPlanView
          onDismiss={onDismiss}
          plan={plan}
          taskId="task-close"
          taskStatus="active"
        />,
      );
    });

    act(() => tree.root.findByProps({ "data-plan-action": "menu" }).props.onClick());
    act(() => tree.root.findByProps({ role: "menuitem" }).props.onClick());

    expect(onDismiss).toHaveBeenCalledOnce();
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
