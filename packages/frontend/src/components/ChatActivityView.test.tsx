import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it } from "vitest";
import type { NormalizedMessage } from "@openaide/app-shell-contracts";
import { ChatActivityView } from "./ChatActivityView";

type ActivityMessage = Extract<NormalizedMessage, { kind: "activity" }>;

describe("ChatActivityView", () => {
  it("renders adjacent bold Thought chunks as separate Markdown blocks", async () => {
    const activity = mixedActivity();
    activity.steps = [{
      kind: "thought",
      message_id: "thought-adjacent-markdown",
      text: "**Assessing signing****Deferring signing****Clarifying trade-offs**",
    }];

    let tree!: ReturnType<typeof create>;
    await act(async () => {
      tree = create(<ChatActivityView activity={activity} taskId="task_1" />);
    });
    act(() => tree.root.findAllByProps({ className: "activity-disclosure-trigger" })[0].props.onClick());
    const paragraphs = tree.root.findAllByType("p");

    expect(paragraphs.map((paragraph) => paragraph.findByType("strong").children.join(""))).toEqual([
      "Assessing signing",
      "Deferring signing",
      "Clarifying trade-offs",
    ]);
  });
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
    const orderedRows = stepList.findAll(
      (node) => node.type === "div" && String(node.props.className).startsWith("activity-step "),
    );
    expect(orderedRows[0].findByType("p").children).toEqual(["Inspect before reading"]);
    expect(orderedRows[1].findByProps({ className: "activity-step-semantic-subject" }).children).toEqual(["notes.md"]);
    expect(orderedRows[2].findByType("p").children).toEqual(["Verify after reading"]);
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

  it("does not expose returned read content in the collapsed activity row", () => {
    const activity: ActivityMessage = {
      kind: "activity",
      id: "activity_cursor_read",
      title: "Read File",
      status: "completed",
      created_at: "2026-07-13T00:00:00Z",
      collapsed: true,
      steps: [
        {
          kind: "tool",
          tool_call_id: "cursor_read",
          name: "read",
          status: "completed",
          output_preview: "fn main() {}",
          permission_outcomes: [],
        },
      ],
    };

    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<ChatActivityView activity={activity} taskId="task_1" />);
    });

    const groupTrigger = tree.root.findAllByProps({ className: "activity-disclosure-trigger" })[0];
    act(() => groupTrigger.props.onClick());

    expect(tree.root.findAllByProps({ className: "activity-step-preview" })).toHaveLength(0);
  });

  it("promotes a Cursor read result's frontmatter name into the rendered title", () => {
    const activity: ActivityMessage = {
      kind: "activity",
      id: "activity_cursor_skill_read",
      title: "Read File",
      status: "completed",
      created_at: "2026-07-13T00:00:00Z",
      collapsed: true,
      steps: [
        {
          kind: "tool",
          tool_call_id: "cursor_skill_read",
          name: "read",
          status: "completed",
          output_preview: [
            "---",
            "name: code-review",
            "description: Review changes",
            "---",
          ].join("\n"),
          permission_outcomes: [],
        },
      ],
    };

    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<ChatActivityView activity={activity} taskId="task_1" />);
    });

    const groupTrigger = tree.root.findAllByProps({ className: "activity-disclosure-trigger" })[0];
    act(() => groupTrigger.props.onClick());

    expect(tree.root.findByProps({ className: "activity-step-title" }).children.join("")).toContain(
      "Activated code-review skill",
    );
  });

  it("routes a skill-shaped Cursor read through the skill renderer", () => {
    const skillDocument = [
      "---",
      "name: code-review",
      "description: Review changes",
      "---",
      "",
      "# Code review",
      "",
      "Review the change.",
    ].join("\n");
    const activity: ActivityMessage = {
      kind: "activity",
      id: "activity_cursor_skill_renderer",
      title: "Read File",
      status: "completed",
      created_at: "2026-07-13T00:00:00Z",
      collapsed: true,
      steps: [
        {
          kind: "tool",
          tool_call_id: "cursor_skill_renderer",
          name: "read",
          status: "completed",
          output_preview: skillDocument,
          details: {
            locations: [],
            content: [{ kind: "text", text: skillDocument }],
          },
          permission_outcomes: [],
        },
      ],
    };

    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<ChatActivityView activity={activity} taskId="task_1" />);
    });

    const groupTrigger = tree.root.findAllByProps({ className: "activity-disclosure-trigger" })[0];
    act(() => groupTrigger.props.onClick());
    expect(tree.root.findAllByProps({ className: "activity-step-preview" })).toHaveLength(0);
    const toolTrigger = tree.root.findAllByProps({ className: "activity-disclosure-trigger" })[1];
    act(() => toolTrigger.props.onClick());

    expect(tree.root.find(
      (node) => typeof node.props.className === "string"
        && node.props.className.includes("skill-tool-details"),
    )).toBeDefined();
    expect(tree.root.findAll(
      (node) => node.type === "div"
        && typeof node.props.className === "string"
        && node.props.className.includes("activity-step tool-skill completed"),
    )).toHaveLength(1);
  });

  it("keeps loaded read content out of the compact step when the preview is absent", () => {
    const activity: ActivityMessage = {
      kind: "activity",
      id: "activity_cursor_read_details",
      title: "Read File",
      status: "completed",
      created_at: "2026-07-13T00:00:00Z",
      collapsed: true,
      steps: [
        {
          kind: "tool",
          tool_call_id: "cursor_read_details",
          name: "read",
          status: "completed",
          details: {
            locations: [],
            content: [{ kind: "text", text: "fn main() {}" }],
          },
          permission_outcomes: [],
        },
      ],
    };

    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<ChatActivityView activity={activity} taskId="task_1" />);
    });

    const groupTrigger = tree.root.findAllByProps({ className: "activity-disclosure-trigger" })[0];
    act(() => groupTrigger.props.onClick());

    expect(tree.root.findAllByProps({ className: "activity-step-preview" })).toHaveLength(0);
  });

  it("does not expose command output in the collapsed activity row", () => {
    const activity: ActivityMessage = {
      kind: "activity",
      id: "activity_cursor_execute_output",
      title: "Run command",
      status: "completed",
      created_at: "2026-07-13T00:00:00Z",
      collapsed: true,
      steps: [
        {
          kind: "tool",
          tool_call_id: "cursor_execute_output",
          name: "execute",
          status: "completed",
          input_summary: "git diff",
          output_preview: "Not in allowlist: cat, git diff",
          permission_outcomes: [{
            request_id: "permission_1",
            decision: "approved",
            option_label: "Allow always",
            resolved_at: "2026-07-13T00:00:00Z",
          }],
        },
      ],
    };

    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<ChatActivityView activity={activity} taskId="task_1" />);
    });

    const groupTrigger = tree.root.findAllByProps({ className: "activity-disclosure-trigger" })[0];
    act(() => groupTrigger.props.onClick());

    expect(tree.root.findAllByProps({ className: "activity-step-preview" })).toHaveLength(0);
    expect(JSON.stringify(tree.toJSON())).toContain("Approved");
  });
});

function renderedThoughtRows(tree: ReturnType<typeof create>) {
  return tree.root.findAll(
    (node) => node.type === "div" && node.props.className === "activity-step activity-thought-block",
  );
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
