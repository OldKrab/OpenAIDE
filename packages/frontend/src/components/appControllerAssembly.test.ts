import { describe, expect, it } from "vitest";
import type { TaskSummary } from "@openaide/app-shell-contracts";
import { createInitialState } from "../state/store";
import { appControllerDerivedStateDeps, deriveAppControllerState, visibleTasks } from "./appControllerDerivedState";

describe("deriveAppControllerState", () => {
  it("does not invalidate navigation derivation when only the new task draft changes", () => {
    const state = createInitialState();
    state.tasks = [task({ task_id: "task_1", has_messages: true })];
    state.newTask.prompt = "before";

    const depsBefore = appControllerDerivedStateDeps(state);
    const nextState = {
      ...state,
      newTask: {
        ...state.newTask,
        prompt: "after",
      },
    };

    expect(appControllerDerivedStateDeps(nextState)).toEqual(depsBefore);
  });

  it("returns the active task and filtered visible tasks", () => {
    const state = createInitialState();
    state.activeTaskId = "task_2";
    state.searchQuery = " CODEX ";
    state.tasks = [
      task({ task_id: "task_1", agent_name: "Other", title: "Docs" }),
      task({ task_id: "task_2", agent_name: "Codex", title: "Implementation" }),
    ];

    const derived = deriveAppControllerState(state);

    expect(derived.activeTask?.task_id).toBe("task_2");
    expect(derived.hasActiveTask).toBe(true);
    expect(derived.visibleTasks.map((task) => task.task_id)).toEqual(["task_2"]);
  });

  it("keeps the active task visible when search does not match it", () => {
    const state = createInitialState();
    state.activeTaskId = "task_2";
    state.searchQuery = "no-match-navigation-qa";
    state.tasks = [
      task({ task_id: "task_1", agent_name: "Codex", title: "Matching no-match-navigation-qa" }),
      task({ task_id: "task_2", agent_name: "Codex", title: "Current work" }),
    ];

    const derived = deriveAppControllerState(state);

    expect(derived.visibleTasks.map((item) => item.task_id)).toEqual(["task_1", "task_2"]);
  });

  it("uses shell editor focus for Task Navigation without changing active product state", () => {
    const state = createInitialState();
    state.activeTaskId = "task_1";
    state.tasks = [
      task({ task_id: "task_1", title: "Previously selected" }),
      task({ task_id: "task_2", title: "Focused editor" }),
    ];

    const focused = deriveAppControllerState(state, "task_2");
    const cleared = deriveAppControllerState(state, null);

    expect(focused.activeNavigationTaskId).toBe("task_2");
    expect(focused.activeTask?.task_id).toBe("task_1");
    expect(cleared.activeNavigationTaskId).toBeUndefined();
  });

  it("keeps message-bearing tasks and hides empty placeholder tasks when the search query is blank", () => {
    const tasks = [
      task({ task_id: "task_1", has_messages: true }),
      task({ task_id: "task_2", has_messages: false }),
    ];

    expect(visibleTasks(tasks, "   ").map((visible) => visible.task_id)).toEqual(["task_1"]);
  });

  it("keeps a private empty New Task out of navigation while first Send is in flight", () => {
    const state = createInitialState();
    state.activeTaskId = "task_2";
    state.newTask.submitting = true;
    state.newTask.pending = {
      prompt: "Build the thing",
      context: [],
    };
    state.tasks = [
      task({ task_id: "task_1", has_messages: true }),
      task({ task_id: "task_2", has_messages: false, title: "New task" }),
    ];
    state.taskInputs.task_2 = {
      prompt: "",
      context: [],
      pending: { prompt: "Build the thing", context: [], state: "sending" },
    };

    const derived = deriveAppControllerState(state);

    expect(derived.visibleTasks.map((visible) => visible.task_id)).toEqual(["task_1"]);
  });

  it("keeps a private empty New Task out of navigation after switching tasks", () => {
    const state = createInitialState();
    state.activeTaskId = "task_1";
    state.tasks = [
      task({ task_id: "task_1", has_messages: true, title: "Previous task" }),
      task({ task_id: "task_2", has_messages: false, title: "New task" }),
    ];
    state.taskInputs.task_2 = {
      prompt: "",
      context: [],
      pending: { prompt: "Build the thing", context: [], state: "sending" },
    };

    const derived = deriveAppControllerState(state);

    expect(derived.visibleTasks.map((visible) => visible.task_id)).toEqual(["task_1"]);
  });

  it("does not invent a sidebar row while the New Task remains private", () => {
    const state = createInitialState();
    state.tasks = [
      task({ task_id: "task_1", has_messages: true, title: "Previous task" }),
    ];
    state.newTask.submitting = true;
    state.newTask.pending = {
      prompt: "Fix the visible startup state",
      context: [],
    };
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project_1",
      workspaceLabel: "OpenAIDE",
      workspaceRoot: "/workspace",
    };

    const derived = deriveAppControllerState(state);

    expect(derived.activeNavigationTaskId).toBeUndefined();
    expect(derived.visibleTasks.map((visible) => visible.task_id)).toEqual(["task_1"]);
  });

  it("does not promote a created New Task into navigation before Send is accepted", () => {
    const state = createInitialState();
    state.tasks = [
      task({ task_id: "task_1", has_messages: true, title: "Previous task" }),
    ];
    state.newTask.submitting = true;
    state.newTask.pending = {
      prompt: "Fix the visible startup state",
      context: [],
    };
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project_1",
      workspaceLabel: "OpenAIDE",
      workspaceRoot: "/workspace",
    };

    const beforeCreate = deriveAppControllerState(state);
    state.activeTaskId = "task_new";
    state.tasks = [
      task({
        task_id: "task_new",
        has_messages: false,
        status: "inactive",
        title: "New task",
      }),
      ...state.tasks,
    ];

    const afterCreate = deriveAppControllerState(state);

    expect(beforeCreate.visibleTasks.map((visible) => visible.title)).toEqual(["Previous task"]);
    expect(afterCreate.activeNavigationTaskId).toBeUndefined();
    expect(afterCreate.visibleTasks.map((visible) => visible.task_id)).toEqual(["task_1"]);
  });

  it("matches visible tasks by title, agent name, and status", () => {
    const tasks = [
      task({ task_id: "task_1", title: "Refactor plan", agent_name: "Codex", status: "inactive" }),
      task({ task_id: "task_2", title: "Other", agent_name: "OpenCode", status: "active" }),
      task({ task_id: "task_3", title: "Archive", agent_name: "Other", status: "waiting" }),
    ];

    expect(visibleTasks(tasks, "plan").map((task) => task.task_id)).toEqual(["task_1"]);
    expect(visibleTasks(tasks, "opencode").map((task) => task.task_id)).toEqual(["task_2"]);
    expect(visibleTasks(tasks, "WAITING").map((task) => task.task_id)).toEqual(["task_3"]);
  });
});

function task(overrides: Partial<TaskSummary> = {}): TaskSummary {
  return {
    agent_id: "codex",
    agent_name: "Codex",
    created_at: "2026-05-22T00:00:00.000Z",
    isolation: "local",
    last_activity: "2026-05-22T00:00:00.000Z",
    message_history_version: 1,
    has_messages: true,
    status: "inactive",
    task_id: "task",
    task_version: 1,
    title: "Task",
    unread: false,
    updated_at: "2026-05-22T00:00:00.000Z",
    workspace_root: "/workspace",
    ...overrides,
  };
}
