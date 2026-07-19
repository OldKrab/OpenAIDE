import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it } from "vitest";
import type { NormalizedMessage } from "@openaide/app-shell-contracts";
import { ChatActivityView } from "./ChatActivityView";

type ActivityMessage = Extract<NormalizedMessage, { kind: "activity" }>;

describe("ChatActivityView", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  it("shows up to two Thought rows without a reasoning toggle", () => {
    const activity = mixedActivity();
    activity.steps = activity.steps.slice(0, 3);
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<ChatActivityView activity={activity} taskId="task_1" />);
    });

    const groupTrigger = tree.root.findAllByProps({ className: "activity-disclosure-trigger" })[0];
    act(() => groupTrigger.props.onClick());

    const stepList = tree.root.findByProps({ className: "activity-step-list" });
    expect(stepList.findAllByProps({ className: "activity-reasoning-toggle" })).toHaveLength(0);
    expect(renderedThoughtRows(tree)).toHaveLength(2);
  });

  it("reveals hidden Thought rows in their original activity order", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<ChatActivityView activity={mixedActivity()} taskId="task_1" />);
    });

    const groupTrigger = tree.root.findAllByProps({ className: "activity-disclosure-trigger" })[0];
    act(() => groupTrigger.props.onClick());

    const stepList = tree.root.findByProps({ className: "activity-step-list" });
    const toggle = stepList.findByProps({ className: "activity-reasoning-toggle" });
    expect(toggle.props["aria-expanded"]).toBe(false);
    expect(renderedThoughtRows(tree)).toHaveLength(0);

    act(() => toggle.props.onClick());

    expect(toggle.props["aria-expanded"]).toBe(true);
    expect(renderedThoughtRows(tree)).toHaveLength(3);
    const rendered = JSON.stringify(tree.toJSON());
    expect(rendered.indexOf("Inspect before reading")).toBeLessThan(rendered.indexOf("Read notes.md"));
    expect(rendered.indexOf("Read notes.md")).toBeLessThan(rendered.indexOf("Verify after reading"));
  });

  it("shows every Thought in a Thought-only group without a reasoning toggle", () => {
    const activity = mixedActivity();
    activity.steps = activity.steps.filter((step) => step.kind === "thought");
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<ChatActivityView activity={activity} taskId="task_1" />);
    });

    const groupTrigger = tree.root.findAllByProps({ className: "activity-disclosure-trigger" })[0];
    act(() => groupTrigger.props.onClick());

    const stepList = tree.root.findByProps({ className: "activity-step-list" });
    expect(stepList.findAllByProps({ className: "activity-reasoning-toggle" })).toHaveLength(0);
    expect(renderedThoughtRows(tree)).toHaveLength(3);
  });

  it("shows aggregate tool approval with individual decisions in details", () => {
    const activity = mixedActivity();
    const tool = activity.steps[1];
    if (tool?.kind !== "tool") throw new Error("fixture tool missing");
    tool.permission_outcomes = [
      {
        request_id: "permission_1",
        decision: "approved",
        option_id: "allow_once",
        option_label: "Allow once",
        resolved_at: "2026-07-13T00:00:01Z",
      },
      {
        request_id: "permission_2",
        decision: "rejected",
        option_id: "reject_once",
        option_label: "Reject",
        resolved_at: "2026-07-13T00:00:02Z",
      },
    ];
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<ChatActivityView activity={activity} taskId="task_1" />);
    });

    const triggers = tree.root.findAllByProps({ className: "activity-disclosure-trigger" });
    act(() => triggers[0].props.onClick());
    const collapsed = JSON.stringify(tree.toJSON());
    expect(collapsed).toContain("Approved · Rejected");

    const toolTrigger = tree.root.findAllByProps({ className: "activity-disclosure-trigger" })[1];
    act(() => toolTrigger.props.onClick());
    const rendered = JSON.stringify(tree.toJSON());
    expect(rendered).toContain("Allow once");
    expect(rendered).toContain("Reject");
  });

  it.each([
    ["command", {
      kind: "command" as const,
      command_label: "npm test",
      status: "completed" as const,
      exit_code: 0,
    }, "Ran command"],
    ["thought", {
      kind: "thought" as const,
      message_id: "thought-clean",
      text: "Check the result",
    }, "Thought"],
  ])("keeps successful %s group titles free of a redundant Completed label", (_kind, step, summary) => {
    const activity: ActivityMessage = {
      kind: "activity",
      id: "activity_clean",
      title: "Activity",
      status: "completed",
      created_at: "2026-07-13T00:00:00Z",
      collapsed: true,
      steps: [step],
    };
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<ChatActivityView activity={activity} taskId="task_1" />);
    });
    const trigger = tree.root.findAllByProps({ className: "activity-disclosure-trigger" })[0];

    expect(renderedText(trigger)).toContain(summary);
    expect(trigger.findAllByType("small")).toHaveLength(0);
  });

  it.each([
    ["interrupted", "Interrupted"],
    ["error", "Failed"],
  ])("keeps %s group outcomes explicit", (status, label) => {
    const activity: ActivityMessage = {
      kind: "activity",
      id: `activity_${status}`,
      title: "Command",
      status: status as never,
      created_at: "2026-07-13T00:00:00Z",
      collapsed: true,
      steps: [{ kind: "command", command_label: "npm test", status: status as never, exit_code: 1 }],
    };
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<ChatActivityView activity={activity} taskId="task_1" />);
    });
    const trigger = tree.root.findAllByProps({ className: "activity-disclosure-trigger" })[0];

    expect(trigger.findByType("small").children).toEqual([label]);
  });

  it.each([
    ["running", "Running", "Running"],
    ["completed", "Completed", "Ran"],
    ["interrupted", "Interrupted", "Interrupted"],
    ["error", "Failed", "Failed"],
    ["future_status", "Unknown", "Unknown"],
  ])("renders authoritative %s outer and command labels", (status, outer, command) => {
    const activity: ActivityMessage = {
      kind: "activity",
      id: "activity_status",
      title: "Command",
      status: status as never,
      created_at: "2026-07-13T00:00:00Z",
      collapsed: false,
      steps: [{
        kind: "command",
        command_label: "npm test",
        status: status as never,
        exit_code: status === "error" ? 0 : 9,
      }],
    };
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<ChatActivityView activity={activity} taskId="task_1" />);
    });
    const rendered = JSON.stringify(tree.toJSON());

    expect(rendered).toContain(outer);
    expect(rendered).toContain(command);
    expect(rendered).toContain(status === "error" ? "exit 0" : "exit 9");
    expect(tree.root.findByProps({
      className: `activity-group ${status === "error" ? "failed" : status === "future_status" ? "unknown" : status}`,
    })).toBeDefined();
    if (status !== "completed") {
      expect(rendered).not.toContain("Ran command");
      expect(rendered).not.toContain(">Ran<");
    }
  });
});

function renderedThoughtRows(tree: ReturnType<typeof create>) {
  return tree.root.findAll(
    (node) => node.type === "div" && node.props.className === "activity-step activity-thought-block",
  );
}

function renderedText(root: ReturnType<typeof create>["root"]) {
  return root.findAll(() => true).flatMap((node) => (
    node.children.filter((child): child is string => typeof child === "string")
  )).join(" ");
}

function mixedActivity(): ActivityMessage {
  return {
    kind: "activity",
    id: "activity_1",
    title: "Tool activity",
    status: "completed",
    created_at: "2026-07-13T00:00:00Z",
    collapsed: true,
    steps: [
      { kind: "thought", message_id: "thought_1", text: "Inspect before reading" },
      { kind: "tool", tool_call_id: "tool_1", name: "read", status: "completed", input_summary: "notes.md", permission_outcomes: [] },
      { kind: "thought", message_id: "thought_2", text: "Verify after reading" },
      { kind: "thought", message_id: "thought_3", text: "Prepare the result" },
    ],
  };
}
