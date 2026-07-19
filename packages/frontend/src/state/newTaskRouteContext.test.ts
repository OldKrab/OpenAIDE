import { describe, expect, it } from "vitest";
import { appReducer } from "./appReducer";
import { createInitialState, type AppState } from "./store";
import {
  newTaskRouteWorkspaceReady,
  newTaskWorkspaceActionForRoute,
} from "./newTaskRouteContext";

describe("New Task route context", () => {
  it("leaves native-shell worktree selection under host ownership", () => {
    const state = routeState();
    state.newTask.selection.worktreeId = "worktree_1";

    expect(newTaskWorkspaceActionForRoute({
      surface: "task",
      shell: { kind: "vscodeExtension", navigationMode: "currentProject" },
      projectId: "project_1",
    }, state, true)).toBeUndefined();
    expect(newTaskRouteWorkspaceReady({
      surface: "task",
      shell: { kind: "vscodeExtension", navigationMode: "currentProject" },
      projectId: "project_1",
    }, state, true)).toBe(true);
  });

  it("keeps a project-only reload loading until Project inventory is authoritative", () => {
    let state = createInitialState();
    const route = webRoute({ projectId: "project_1" });

    const action = newTaskWorkspaceActionForRoute(route, state, false);
    expect(action).toEqual({
      type: "newTask:worktree",
      projectId: "project_1",
      worktreeId: undefined,
      label: "Loading workspace",
      path: "",
      resolution: "loading",
    });
    if (!action) throw new Error("Expected loading route state");
    state = appReducer(state, action);
    expect(newTaskRouteWorkspaceReady(route, state, false)).toBe(false);
  });

  it("keeps a project-only history route loading before resolving its Project root", () => {
    let state = routeState();
    state.projectsLoaded = false;
    const route = webRoute({ projectId: "project_2" });

    const loading = newTaskWorkspaceActionForRoute(route, state, false);
    expect(loading).toMatchObject({
      projectId: "project_2",
      label: "Loading workspace",
      resolution: "loading",
    });
    if (!loading) throw new Error("Expected loading history route state");
    state = appReducer(state, loading);
    expect(newTaskRouteWorkspaceReady(route, state, false)).toBe(false);

    state.projects = [{
      projectId: "project_2",
      label: "Second Project",
      workspaceRoot: "/workspace/second",
    }];
    state.projectsLoaded = true;
    const resolved = newTaskWorkspaceActionForRoute(route, state, true);
    expect(resolved).toEqual({
      type: "newTask:worktree",
      worktreeId: undefined,
      label: "Project root",
      path: "/workspace/second",
      resolution: undefined,
    });
    if (!resolved) throw new Error("Expected resolved history route state");
    state = appReducer(state, resolved);
    expect(newTaskRouteWorkspaceReady(route, state, true)).toBe(true);
  });

  it("marks a missing project-only route unavailable after authoritative inventory", () => {
    let state = createInitialState();
    state.projectsLoaded = true;
    const route = webRoute({ projectId: "project_missing" });

    const unavailable = newTaskWorkspaceActionForRoute(route, state, true);
    expect(unavailable).toEqual({
      type: "newTask:worktree",
      projectId: "project_missing",
      worktreeId: undefined,
      label: "Workspace unavailable",
      path: "",
      resolution: "unavailable",
    });
    if (!unavailable) throw new Error("Expected unavailable route state");
    state = appReducer(state, unavailable);
    expect(newTaskRouteWorkspaceReady(route, state, true)).toBe(false);
  });

  it("represents routed worktree identity before inventory arrives without exposing Project root", () => {
    let state = createInitialState();
    const route = {
      surface: "task",
      shell: { kind: "web", navigationMode: "project" },
      projectId: "project_1",
      worktreeId: "worktree_1",
    } as never;
    state.newTask.selection.projectId = "project_old";

    const beforeProjects = newTaskWorkspaceActionForRoute(route, state, false);
    expect(beforeProjects).toEqual({
      type: "newTask:worktree",
      projectId: "project_1",
      worktreeId: "worktree_1",
      label: "Loading workspace",
      path: "",
      resolution: "loading",
    });
    expect(beforeProjects).not.toMatchObject({ label: "Project root" });
    if (!beforeProjects) throw new Error("Expected routed worktree action");
    state = appReducer(state, beforeProjects);
    expect(state.newTask.selection).toMatchObject({
      projectId: "project_1",
      worktreeId: "worktree_1",
      workspaceLabel: "Loading workspace",
      workspaceRoot: "",
    });
    expect(newTaskRouteWorkspaceReady(route, state, false)).toBe(false);

    state.projects = [{
      projectId: "project_1",
      label: "Project",
      workspaceRoot: "/workspace/project",
      worktreeRepositoryId: "repository_1",
    }];
    state.projectsLoaded = true;
    expect(newTaskWorkspaceActionForRoute(route, state, false)).toBeUndefined();
    expect(state.newTask.selection.workspaceLabel).not.toBe("Project root");
    expect(newTaskRouteWorkspaceReady(route, state, false)).toBe(false);

    state.worktreeRepositories.repository_1 = routeRepository();
    expect(newTaskWorkspaceActionForRoute(route, state, true)).toEqual({
      type: "newTask:worktree",
      worktreeId: "worktree_1",
      label: "QA fixes",
      path: "/workspace/qa-fixes",
      resolution: undefined,
    });
    state.newTask.selection.workspaceLabel = "QA fixes";
    state.newTask.selection.workspaceRoot = "/workspace/qa-fixes";
    state.newTask.workspaceResolution = undefined;
    expect(newTaskRouteWorkspaceReady(route, state, true)).toBe(true);
  });

  it("reconstructs a stable worktree identity from a deep link", () => {
    const state = routeState();

    expect(newTaskWorkspaceActionForRoute(
      webRoute({ projectId: "project_1", worktreeId: "worktree_1" }),
      state,
      true,
    )).toEqual({
      type: "newTask:worktree",
      worktreeId: "worktree_1",
      label: "QA fixes",
      path: "/workspace/qa-fixes",
      resolution: undefined,
    });
  });

  it("keeps an invalid routed worktree explicit instead of falling back to Project root", () => {
    const state = routeState();

    expect(newTaskWorkspaceActionForRoute(
      webRoute({ projectId: "project_1", worktreeId: "worktree_missing" }),
      state,
      true,
    )).toEqual({
      type: "newTask:worktree",
      worktreeId: "worktree_missing",
      label: "Workspace unavailable",
      path: "",
      resolution: "unavailable",
    });
  });

  it("restores Project root only when browser history explicitly selects its route", () => {
    const state = routeState();
    state.newTask.selection.worktreeId = "worktree_1";
    state.newTask.selection.workspaceRoot = "/workspace/qa-fixes";

    expect(newTaskWorkspaceActionForRoute(
      webRoute({ projectId: "project_1" }),
      state,
      true,
    )).toEqual({
      type: "newTask:worktree",
      worktreeId: undefined,
      label: "Project root",
      path: "/workspace/project",
      resolution: undefined,
    });
  });

  it("clears retained worktree identity when browser history returns to the root route", () => {
    const state = routeState();
    state.newTask.selection.worktreeId = "worktree_1";
    state.newTask.selection.workspaceLabel = "QA fixes";
    state.newTask.selection.workspaceRoot = "/workspace/qa-fixes";
    const rootRoute = webRoute({});

    expect(newTaskWorkspaceActionForRoute(rootRoute, state, true)).toEqual({
      type: "newTask:worktree",
      worktreeId: undefined,
      label: "Project root",
      path: "/workspace/project",
      resolution: undefined,
    });
    state.newTask.selection.worktreeId = undefined;
    state.newTask.selection.workspaceLabel = "Project root";
    state.newTask.selection.workspaceRoot = "/workspace/project";
    expect(newTaskRouteWorkspaceReady(rootRoute, state, true)).toBe(true);
  });

  it("never borrows retained Project context for a worktree URL without explicit Project identity", () => {
    const state = routeState();
    state.newTask.selection.workspaceLabel = "Retained Project";
    const route = webRoute({ worktreeId: "worktree_1" });

    expect(newTaskWorkspaceActionForRoute(route, state, false)).toEqual({
      type: "newTask:worktree",
      projectId: null,
      worktreeId: "worktree_1",
      label: "Loading workspace",
      path: "",
      resolution: "loading",
    });
    expect(newTaskRouteWorkspaceReady(route, state, false)).toBe(false);

    const unavailable = newTaskWorkspaceActionForRoute(route, state, true);
    expect(unavailable).toEqual({
      type: "newTask:worktree",
      projectId: null,
      worktreeId: "worktree_1",
      label: "Workspace unavailable",
      path: "",
      resolution: "unavailable",
    });
    if (!unavailable) throw new Error("Expected explicit unavailable route state");
    const reconciled = appReducer(state, unavailable);
    expect(reconciled.newTask.selection.projectId).toBeUndefined();
    expect(reconciled.newTask.selection.worktreeId).toBe("worktree_1");
    expect(reconciled.newTask.workspaceResolution).toBe("unavailable");
    expect(newTaskRouteWorkspaceReady(route, reconciled, true)).toBe(false);
  });

  it("moves a missing routed Project from loading to unavailable and recovers when inventory changes", () => {
    const state = createInitialState();
    state.projectsLoaded = true;
    const route = webRoute({ projectId: "project_later", worktreeId: "worktree_later" });

    expect(newTaskWorkspaceActionForRoute(route, state, false)).toMatchObject({
      projectId: "project_later",
      worktreeId: "worktree_later",
      resolution: "loading",
    });
    expect(newTaskWorkspaceActionForRoute(route, state, true)).toMatchObject({
      projectId: "project_later",
      worktreeId: "worktree_later",
      resolution: "unavailable",
    });
    expect(newTaskRouteWorkspaceReady(route, state, true)).toBe(false);

    state.projects = [{
      projectId: "project_later",
      label: "Later Project",
      workspaceRoot: "/workspace/later",
      worktreeRepositoryId: "repository_later",
    }];
    state.worktreeRepositories.repository_later = {
      ...routeRepository(),
      repositoryId: "repository_later" as never,
      worktrees: [{
        ...routeRepository().worktrees[0],
        worktreeId: "worktree_later" as never,
      }],
    };
    state.newTask.selection.projectId = "project_later";
    state.newTask.selection.worktreeId = "worktree_later";
    state.newTask.selection.workspaceLabel = "Workspace unavailable";
    state.newTask.workspaceResolution = "unavailable";

    expect(newTaskWorkspaceActionForRoute(route, state, true)).toMatchObject({
      label: "QA fixes",
      path: "/workspace/qa-fixes",
      resolution: undefined,
    });
    state.newTask.selection.workspaceLabel = "QA fixes";
    state.newTask.selection.workspaceRoot = "/workspace/qa-fixes";
    state.newTask.workspaceResolution = undefined;
    expect(newTaskRouteWorkspaceReady(route, state, true)).toBe(true);
  });
});

function routeState() {
  const state = createInitialState();
  state.projectsLoaded = true;
  state.projects = [{
    projectId: "project_1",
    label: "Project",
    workspaceRoot: "/workspace/project",
    worktreeRepositoryId: "repository_1",
  }];
  state.newTask.selection.projectId = "project_1";
  state.worktreeRepositories.repository_1 = routeRepository();
  return state;
}

function webRoute(route: { projectId?: string; worktreeId?: string }) {
  return {
    surface: "task" as const,
    shell: { kind: "web" as const, navigationMode: "project" as const },
    ...route,
  };
}

function routeRepository(): AppState["worktreeRepositories"][string] {
  return {
    repositoryId: "repository_1" as never,
    revision: 1,
    worktrees: [{
      worktreeId: "worktree_1" as never,
      name: "QA fixes",
      path: "/workspace/qa-fixes",
      kind: "managed",
      availability: "available",
      forgotten: false,
      head: { kind: "branch", name: "qa/fixes" },
      linkedTaskCount: 0,
      runningTaskCount: 0,
    }],
    operations: [],
  } as never;
}
