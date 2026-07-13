import { describe, expect, it } from "vitest";
import { createInitialState } from "../state/store";
import { projectIdForWorkspaceRoot } from "../state/projectIdentity";
import {
  newTaskConfigOptionsContextForRequests,
  newTaskProjectIdForRequests,
} from "./newTaskRequestContext";

describe("new Task request context", () => {
  it("does not resurrect a Project solely from historical Tasks", () => {
    const state = createInitialState();
    state.newTask.selection.projectId = "project-removed";
    state.tasks = [{ project_id: "project-removed" } as never];

    expect(newTaskProjectIdForRequests(state, undefined)).toBeUndefined();
  });

  it("accepts the routed Project while the authoritative Project snapshot loads", () => {
    const state = createInitialState();
    state.newTask.selection.projectId = "project-routed";

    expect(newTaskProjectIdForRequests(state, "project-routed")).toBe("project-routed");
  });

  it("uses a configured Project from the authoritative collection", () => {
    const state = createInitialState();
    state.newTask.selection.projectId = "project-configured";
    state.projects = [{ projectId: "project-configured", label: "Configured" }];

    expect(newTaskProjectIdForRequests(state, undefined)).toBe("project-configured");
  });

  it("uses workspace context only for an unpersisted deterministic Project identity", () => {
    const state = createInitialState();
    state.newTask.selection.workspaceRoot = "/workspace/new";
    state.newTask.selection.projectId = projectIdForWorkspaceRoot("/workspace/new");

    expect(newTaskConfigOptionsContextForRequests(state, undefined)).toEqual({
      projectId: projectIdForWorkspaceRoot("/workspace/new"),
      workspaceRoot: "/workspace/new",
    });
  });
});
