import { describe, expect, it } from "vitest";
import type { NormalizedMessage } from "@openaide/app-shell-contracts";
import {
  activityStatusLabel,
  activityStepCompletedLabel,
  activityStepContext,
  activityStepLabel,
  activityStepProgressLabel,
  activityStepStatus,
  activitySummary,
} from "./activityLabels";

describe("activity labels", () => {
  it("presents interrupted work without calling it completed or failed", () => {
    expect(activityStatusLabel("interrupted")).toBe("Interrupted");
    expect(activityStepStatus({ kind: "tool", name: "edit", status: "interrupted" })).toBe("Interrupted");
  });
  it("turns old generic exec tool rows into command labels", () => {
    expect(
      activitySummary(
        activity("exec_command", "completed", [{ kind: "tool", name: "other", status: "completed" }]),
      ),
    ).toBe("Ran command");
  });

  it("uses runtime-provided tool input summaries as the primary subject", () => {
    const message = activity("exec_command", "completed", [
      { kind: "tool", name: "execute", status: "completed", input_summary: "ssh oldserver cat authorized_keys" },
    ]);

    expect(activitySummary(message)).toBe("Ran command");
    expect(activityStepLabel(message.steps[0])).toBe("ssh oldserver cat authorized_keys");
  });

  it("shows typed tool action and subject for inspectable rows", () => {
    expect(
      activityStepLabel({
        kind: "tool",
        name: "edit",
        status: "completed",
        details: {
          locations: [],
          content: [{ kind: "diff", path: "/workspace/src/activityLabels.ts", new_text: "changed" }],
        },
      }),
    ).toBe("Edit activityLabels.ts");
    expect(
      activityStepLabel({
        kind: "tool",
        name: "search",
        status: "completed",
        input_summary: "/workspace",
        details: {
          locations: [],
          content: [],
          input: { command: ["rg", "-n", "activity", "."], cwd: "/workspace", query: "activity", fields: [] },
        },
      }),
    ).toBe("Search “activity” in /workspace");
    expect(activityStepLabel({ kind: "tool", name: "read", status: "completed", input_summary: "Read notes.md" })).toBe(
      "Read notes.md",
    );
  });

  it("uses command details when saved summaries only contain cwd", () => {
    const message = activity("Search index.md in .", "completed", [
      {
        kind: "tool",
        name: "search",
        status: "completed",
        input_summary: "sample-workspace",
        details: {
          locations: [],
          content: [],
          input: {
            command: ["zsh", "-lc", "find . -name 'index.md' -print"],
            cwd: "sample-workspace",
            fields: [],
          },
        },
      },
    ]);

    expect(activitySummary(message)).toBe("Ran search");
    expect(activityStepLabel(message.steps[0])).toBe("Search find . -name 'index.md' -print in sample-workspace");
    expect(activityStepContext(message.steps[0])).toBeUndefined();
  });

  it("delimits a fallback ACP regex while keeping scope inline", () => {
    const step = {
      kind: "tool" as const,
      name: "search",
      status: "completed" as const,
      input_summary: "Search for '\"name\":\"search\"[^\\n]{0,500}' in state",
    };

    expect(activityStepLabel(step)).toBe('Search /"name":"search"[^\\n]{0,500}/ in state');
    expect(activityStepContext(step)).toBeUndefined();
  });

  it("bounds long regex previews without changing their separate scope", () => {
    const step = {
      kind: "tool" as const,
      name: "search",
      status: "completed" as const,
      input_summary:
        "Search for 'task_create_attach_failure|follow_up_attach_failure|no rollout|missing.*session|Agent work stopped' in tests",
    };

    expect(activityStepLabel(step)).toBe("Search /task_create_attach_failure|follow_up_attach_fai…/ in tests");
    expect(activityStepContext(step)).toBeUndefined();
  });

  it("does not call ordinary punctuation in a literal query a regex", () => {
    const step = {
      kind: "tool" as const,
      name: "search",
      status: "completed" as const,
      input_summary: "Search for 'activityLabels.test.ts' in frontend",
    };

    expect(activityStepLabel(step)).toBe("Search “activityLabels.test.ts” in frontend");
    expect(activityStepContext(step)).toBeUndefined();
  });

  it("labels terminal input separately from commands", () => {
    expect(
      activitySummary(
        activity("write_stdin", "completed", [{ kind: "tool", name: "other", status: "completed" }]),
      ),
    ).toBe("Sent terminal input");
  });

  it("keeps protocol kinds readable for non-command tools", () => {
    expect(
      activitySummary(
        activity("Search files", "completed", [
          { kind: "tool", name: "search", status: "completed", input_summary: "workspace_root" },
        ]),
      ),
    ).toBe("Ran search");
  });

  it("gives every defined ACP kind its own action and grouped classification", () => {
    const steps = [
      { kind: "tool" as const, name: "delete", status: "completed" as const, input_summary: "old.ts" },
      { kind: "tool" as const, name: "move", status: "completed" as const, input_summary: "new.ts" },
      { kind: "tool" as const, name: "think", status: "completed" as const },
      { kind: "tool" as const, name: "fetch", status: "completed" as const, input_summary: "docs" },
      { kind: "tool" as const, name: "switch_mode", status: "completed" as const, input_summary: "Plan" },
    ];

    expect(steps.map(activityStepLabel)).toEqual([
      "Delete old.ts",
      "Move new.ts",
      "Reasoning tool",
      "Fetch docs",
      "Switch mode to Plan",
    ]);
    expect(activitySummary(activity("Tool activity", "completed", steps))).toBe(
      "Deleted file, moved file, used reasoning tool, fetched resource, switched mode",
    );
  });

  it("keeps id-only web search rows free of protocol identifiers", () => {
    const running = { kind: "tool" as const, name: "web_search", status: "running" as const };
    const completed = { ...running, status: "completed" as const };

    expect(activityStepLabel(running)).toBe("Web search");
    expect(activityStepProgressLabel(running)).toBe("Searching the web");
    expect(activityStepCompletedLabel(completed)).toBe("Searched the web");
  });

  it("presents agent-coordination tools as product actions instead of protocol names", () => {
    const message = activity("spawn_agent", "completed", [
      { kind: "tool", name: "other", status: "completed", input_summary: "name spawn_agent" },
    ]);

    expect(activitySummary(message)).toBe("Coordinated subagent");
    expect(activityStepLabel(message.steps[0])).toBe("Started subagent");
  });

  it("summarizes grouped command activity without promoting every command", () => {
    expect(
      activitySummary(
        activity("Commands", "completed", [
          { kind: "tool", name: "execute", status: "completed", input_summary: "git status --short" },
          { kind: "tool", name: "execute", status: "completed", input_summary: "npm run check" },
        ]),
      ),
    ).toBe("Ran 2 commands");
  });

  it("does not classify execute tools as searches from words in their commands", () => {
    expect(
      activitySummary(
        activity("Commands", "completed", [
          {
            kind: "tool",
            name: "execute",
            status: "completed",
            input_summary: "playwright-cli -s=search-title open 127.0.0.1:5574",
          },
        ]),
      ),
    ).toBe("Ran command");
  });

  it("uses trusted execute presentation for semantic titles and status copy", () => {
    const skill = {
      kind: "tool" as const,
      name: "execute",
      status: "completed" as const,
      presentation: { kind: "skill" as const, subjects: ["tdd", "diagnosing-bugs", "impeccable"] },
      input_summary: "sed -n ...",
    };
    const search = {
      kind: "tool" as const,
      name: "execute",
      status: "running" as const,
      presentation: { kind: "search" as const, subjects: ["activityLabels in frontend"] },
      input_summary: "rg -n activityLabels frontend",
    };

    expect(activitySummary(activity("Commands", "completed", [skill]))).toBe("Activated skill");
    expect(activityStepLabel(skill)).toBe("Activated tdd, diagnosing-bugs, and impeccable skills");
    expect(activityStepProgressLabel({ ...skill, status: "running" })).toBe(
      "Activating tdd, diagnosing-bugs, and impeccable skills",
    );
    expect(activityStepCompletedLabel(skill)).toBe("Activated tdd, diagnosing-bugs, and impeccable skills");
    expect(activityStepLabel(search)).toBe("Search activityLabels in frontend");
    expect(activityStepProgressLabel(search)).toBe("Searching activityLabels in frontend");
  });

  it("summarizes mixed grouped tool activity by work type", () => {
    expect(
      activitySummary(
        activity("Tool activity", "completed", [
          { kind: "tool", name: "read", status: "completed", input_summary: "App.tsx" },
          { kind: "tool", name: "read", status: "completed", input_summary: "app.css" },
          { kind: "tool", name: "edit", status: "completed", input_summary: "chatPaging.ts" },
          { kind: "tool", name: "execute", status: "completed", input_summary: "npm run check" },
        ]),
      ),
    ).toBe("Read 2 files, updated file, ran command");
  });

  it("presents activated skills distinctly inside mixed activity", () => {
    const message = activity("Tool activity", "completed", [
      { kind: "tool", name: "skill", status: "completed", input_summary: "tdd" },
      { kind: "tool", name: "read", status: "completed", input_summary: "PRODUCT.md" },
      { kind: "tool", name: "skill", status: "completed", input_summary: "impeccable" },
      { kind: "tool", name: "execute", status: "completed", input_summary: "npm test" },
    ]);

    expect(activitySummary(message)).toBe("Activated 2 skills, read file, ran command");
    expect(activityStepLabel(message.steps[0])).toBe("Activated tdd skill");
    expect(activityStepProgressLabel(message.steps[0])).toBe("Activating tdd skill");
    expect(activityStepCompletedLabel(message.steps[0])).toBe("Activated tdd skill");
    expect(activityStepLabel(message.steps[2])).toBe("Activated impeccable skill");
  });

  it("classifies generic tool rows from their visible summaries", () => {
    expect(
      activitySummary(
        activity("Tool activity", "completed", [
          { kind: "tool", name: "other", status: "completed", input_summary: "Read file '/workspace/a.ts'" },
          { kind: "tool", name: "other", status: "completed", input_summary: "Read file '/workspace/b.ts'" },
          { kind: "tool", name: "other", status: "completed", input_summary: "Searched for \"activity\"" },
          { kind: "tool", name: "other", status: "completed", input_summary: "Updated src/activity.ts" },
        ]),
      ),
    ).toBe("Read 2 files, ran search, updated file");
  });

  it("classifies tool-like text rows from their visible labels", () => {
    expect(
      activitySummary(
        activity("Tool activity", "completed", [
          { kind: "text", text: "Read file '/workspace/a.ts'" },
          { kind: "text", text: "Read file '/workspace/b.ts'" },
          { kind: "text", text: "/usr/bin/zsh -lc \"sed -n '1,180p' packages/frontend/src/state/activityLabels.ts\"" },
        ]),
      ),
    ).toBe("Read 2 files, ran command");

    expect(activitySummary(activity("Editing files", "completed", [{ kind: "text", text: "Editing files" }]))).toBe("Updated file");
  });

  it("omits the count for single grouped actions and includes thoughts in order", () => {
    expect(
      activitySummary(
        activity("Tool activity", "completed", [
          { kind: "thought", text: "Need current files first." },
          { kind: "tool", name: "read", status: "completed", input_summary: "PRODUCT.md" },
          { kind: "tool", name: "search", status: "completed", input_summary: "tool activity" },
        ]),
      ),
    ).toBe("Thought, read file, ran search");

    expect(
      activitySummary(
        activity("Tool activity", "completed", [
          { kind: "tool", name: "read", status: "completed", input_summary: "PRODUCT.md" },
          { kind: "thought", text: "Keep it deterministic." },
          { kind: "thought", text: "No LLM summary." },
        ]),
      ),
    ).toBe("Read file, thought twice");

    expect(
      activitySummary(
        activity("Tool activity", "completed", [
          { kind: "thought", text: "Check the wording." },
          { kind: "thought", text: "Compare the alternatives." },
          { kind: "thought", text: "Choose the clearest label." },
        ]),
      ),
    ).toBe("Thought 3 times");
  });

  it("uses simple past for completed thought rows and status", () => {
    const thought = { kind: "thought" as const, text: "Choose consistent tense." };

    expect(activityStepLabel(thought)).toBe("Thought");
    expect(activityStepCompletedLabel(thought)).toBe("Thought");
  });
});

function activity(
  title: string,
  status: Extract<NormalizedMessage, { kind: "activity" }>["status"],
  steps: Extract<NormalizedMessage, { kind: "activity" }>["steps"],
): Extract<NormalizedMessage, { kind: "activity" }> {
  return {
    kind: "activity",
    id: `activity:${title}`,
    title,
    status,
    created_at: "2026-05-19T00:00:00Z",
    collapsed: true,
    steps,
  };
}
