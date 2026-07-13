import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { TaskHeader } from "./TaskHeader";

describe("TaskHeader", () => {
  it("can omit redundant workspace context in an editor shell", () => {
    const html = renderToStaticMarkup(
      <TaskHeader agentId="codex" agentName="Codex" showWorkspaceContext={false} status="inactive" title="Task" workspaceRoot="/workspace/app" />,
    );

    expect(html).not.toContain("task-header-workspace");
  });

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

  it("names the waiting state directly", () => {
    const html = renderToStaticMarkup(
      <TaskHeader agentId="codex" agentName="Codex" status="waiting" title="Waiting task" workspaceRoot="" />,
    );

    expect(html).toContain('aria-label="Task status: Waiting"');
    expect(html).not.toContain("Workspace");
  });
});
