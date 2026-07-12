import { describe, expect, it } from "vitest";

import {
  readRetainedNewTaskContext,
  retainNewTaskContext,
  selectInitialNewTaskContext,
} from "./newTaskSelectionDefaults";

const projects = [
  { projectId: "project-a", label: "A" },
  { projectId: "project-b", label: "B" },
];
const agents = [
  { agentId: "codex" as never, label: "Codex", status: "connected" as const },
  { agentId: "opencode" as never, label: "OpenCode", status: "connected" as const },
];

describe("New Task initial selection", () => {
  it("keeps valid client choices before shell and App Server defaults", () => {
    expect(selectInitialNewTaskContext({
      retained: { projectId: "project-b", agentId: "opencode" },
      shellProjectId: "project-a",
      defaults: { projectId: "project-a" as never, agentId: "codex" as never },
      projects,
      agents,
    })).toEqual({ projectId: "project-b", agentId: "opencode" });
  });

  it("uses the shell Project and App Server Agent default without retained choices", () => {
    expect(selectInitialNewTaskContext({
      shellProjectId: "project-b",
      defaults: { projectId: "project-a" as never, agentId: "opencode" as never },
      projects,
      agents,
    })).toEqual({ projectId: "project-b", agentId: "opencode" });
  });

  it("ignores missing choices and deterministically uses the first available entries", () => {
    expect(selectInitialNewTaskContext({
      retained: { projectId: "removed-project", agentId: "removed-agent" },
      defaults: { projectId: "removed-project" as never, agentId: "removed-agent" as never },
      projects,
      agents,
    })).toEqual({ projectId: "project-a", agentId: "codex" });
  });

  it("does not erase retained ids while initialization is still missing a choice", () => {
    const values = new Map<string, string>();
    const storage = {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => { values.set(key, value); },
    };
    retainNewTaskContext("root", "client", { projectId: "project-b", agentId: "opencode" }, storage);

    retainNewTaskContext("root", "client", { projectId: undefined, agentId: undefined }, storage);

    expect(readRetainedNewTaskContext("root", "client", storage)).toEqual({
      projectId: "project-b",
      agentId: "opencode",
    });
  });
});
