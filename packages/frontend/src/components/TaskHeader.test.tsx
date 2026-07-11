import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { TaskHeader } from "./TaskHeader";

describe("TaskHeader", () => {
  it("keeps the task title concise while exposing agent, workspace, and running state", () => {
    const html = renderToStaticMarkup(
      <TaskHeader
        agentId="codex"
        agentName="Codex"
        status="active"
        title="Review all project code and fix the highest-impact defects"
        workspaceRoot="/workspace/OpenAIDE"
      />,
    );

    expect(html).toContain("Review all project code and fix the highest-impact defects");
    expect(html).toContain('title="Review all project code and fix the highest-impact defects"');
    expect(html).toContain('aria-label="Task status: Running"');
    expect(html).toContain("OpenAIDE");
    expect(html).toContain("Codex");
    expect(html.indexOf("Task status: Running")).toBeLessThan(html.indexOf("Codex"));
    expect(html.indexOf("Codex")).toBeLessThan(html.indexOf("OpenAIDE"));
  });

  it("uses explicit attention copy for blocked tasks", () => {
    const html = renderToStaticMarkup(
      <TaskHeader agentId="codex" agentName="Codex" status="blocked" title="Blocked task" workspaceRoot="" />,
    );

    expect(html).toContain('aria-label="Task status: Needs attention"');
    expect(html).not.toContain("Workspace");
  });
});
