import { describe, expect, it } from "vitest";
import { createInitialState } from "./store";
import { navigationViewParams } from "./navigationViewParams";
import type { WebviewBootstrap } from "./surfaceTypes";

const vscodeNavigation = (projectIds?: string[], projectId?: string): WebviewBootstrap => ({
  surface: "navigation",
  shell: { kind: "vscodeExtension", navigationMode: "currentProject" },
  projectId,
  projectIds,
  appServerConnection: { kind: "webProxy", endpointUrl: "/probe" },
} as unknown as WebviewBootstrap);

const webTask = {
  surface: "task",
  shell: { kind: "web", navigationMode: "allProjects" },
  appServerConnection: { kind: "webProxy", endpointUrl: "/probe" },
} as unknown as WebviewBootstrap;

describe("navigationViewParams", () => {
  it("limits a VS Code view to its workspace Projects", () => {
    const params = navigationViewParams(createInitialState(), vscodeNavigation(["project-1", "project-2"]), "tasks");

    expect(params.projectIds).toEqual(["project-1", "project-2"]);
    // The history limit follows the fixed Projects, not every configured Project.
    expect(params.historyLimit).toBe(7);
  });

  it("falls back to the single workspace Project and leaves other shells unfiltered", () => {
    expect(navigationViewParams(createInitialState(), vscodeNavigation(undefined, "project-1"), "tasks").projectIds)
      .toEqual(["project-1"]);
    expect(navigationViewParams(createInitialState(), webTask, "tasks")).not.toHaveProperty("projectIds");
  });

  it("sizes the history limit by Project count: 20, 7, then 3", () => {
    const withProjects = (count: number) => ({
      ...createInitialState(),
      projects: Array.from({ length: count }, (_, index) => ({ projectId: `project-${index}`, label: "P" })),
    });

    expect(navigationViewParams(withProjects(1), webTask, "tasks").historyLimit).toBe(20);
    expect(navigationViewParams(withProjects(2), webTask, "tasks").historyLimit).toBe(7);
    expect(navigationViewParams(withProjects(5), webTask, "tasks").historyLimit).toBe(3);
  });

  it("carries per-Project overrides in a stable order and the open Task", () => {
    const state = {
      ...createInitialState(),
      activeTaskId: "task-1",
      navigationHistoryLimits: { "project-b": 0, "project-a": 27 },
    };

    const params = navigationViewParams(state, webTask, "archive");

    expect(params.section).toBe("archive");
    expect(params.historyLimits).toEqual([
      { projectId: "project-a", historyLimit: 27 },
      { projectId: "project-b", historyLimit: 0 },
    ]);
    expect(params.openRow).toEqual({ kind: "task", taskId: "task-1" });
  });
});
