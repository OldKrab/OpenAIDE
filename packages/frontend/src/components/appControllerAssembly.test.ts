import { describe, expect, it, vi } from "vitest";
import type { TaskSummary } from "@openaide/app-shell-contracts";
import { createInitialState } from "../state/store";
import { appControllerDerivedStateDeps, deriveAppControllerState, visibleTasks } from "./appControllerDerivedState";
import { requestControllerNativeSessions } from "./appControllerNativeSessions";

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

  it("keeps message-bearing tasks and hides empty placeholder tasks when the search query is blank", () => {
    const tasks = [
      task({ task_id: "task_1", has_messages: true }),
      task({ task_id: "task_2", has_messages: false }),
    ];

    expect(visibleTasks(tasks, "   ").map((visible) => visible.task_id)).toEqual(["task_1"]);
  });

  it("keeps the pending active empty task visible while the first send is in flight", () => {
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
      pending: { prompt: "Build the thing", context: [] },
    };

    const derived = deriveAppControllerState(state);

    expect(derived.visibleTasks.map((visible) => visible.task_id)).toEqual(["task_1", "task_2"]);
    expect(derived.visibleTasks.map((visible) => visible.task_id)).not.toContain("__pending_new_task__");
    expect(derived.visibleTasks[1]).toMatchObject({
      has_messages: true,
      status: "active",
      title: "Build the thing",
    });
  });

  it("keeps a pending empty task visible after switching to another task", () => {
    const state = createInitialState();
    state.activeTaskId = "task_1";
    state.tasks = [
      task({ task_id: "task_1", has_messages: true, title: "Previous task" }),
      task({ task_id: "task_2", has_messages: false, title: "New task" }),
    ];
    state.taskInputs.task_2 = {
      prompt: "",
      context: [],
      pending: { prompt: "Build the thing", context: [] },
    };

    const derived = deriveAppControllerState(state);

    expect(derived.visibleTasks.map((visible) => visible.task_id)).toEqual(["task_1", "task_2"]);
    expect(derived.visibleTasks[1]).toMatchObject({
      has_messages: true,
      status: "active",
      title: "Build the thing",
    });
  });

  it("shows a selected pending new task row before the Backend returns the created task", () => {
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

    expect(derived.activeNavigationTaskId).toBe("__pending_new_task__");
    expect(derived.visibleTasks.map((visible) => visible.task_id)).toEqual(["__pending_new_task__", "task_1"]);
    expect(derived.visibleTasks[0]).toMatchObject({
      agent_id: "codex",
      agent_name: "Codex",
      has_messages: true,
      project_id: "project_1",
      project_label: "OpenAIDE",
      status: "active",
      title: "Fix the visible startup state",
    });
  });

  it("keeps a newly created task stable and in progress until its first send starts", () => {
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

    expect(beforeCreate.visibleTasks.map((visible) => visible.title)).toEqual([
      "Fix the visible startup state",
      "Previous task",
    ]);
    expect(afterCreate.activeNavigationTaskId).toBe("task_new");
    expect(afterCreate.visibleTasks.map((visible) => visible.task_id)).toEqual(["task_new", "task_1"]);
    expect(afterCreate.visibleTasks[0]).toMatchObject({
      has_messages: true,
      status: "active",
      title: "Fix the visible startup state",
    });
  });

  it("matches visible tasks by title, agent name, and status", () => {
    const tasks = [
      task({ task_id: "task_1", title: "Refactor plan", agent_name: "Codex", status: "inactive" }),
      task({ task_id: "task_2", title: "Other", agent_name: "OpenCode", status: "active" }),
      task({ task_id: "task_3", title: "Archive", agent_name: "Other", status: "blocked" }),
    ];

    expect(visibleTasks(tasks, "plan").map((task) => task.task_id)).toEqual(["task_1"]);
    expect(visibleTasks(tasks, "opencode").map((task) => task.task_id)).toEqual(["task_2"]);
    expect(visibleTasks(tasks, "BLOCKED").map((task) => task.task_id)).toEqual(["task_3"]);
  });
});

describe("requestControllerNativeSessions", () => {
  it("increments request ids and reports an error when listing sessions without BackendConnection", () => {
    const dispatch = vi.fn();
    const latestSessionListRequestId = { current: undefined as number | undefined };
    const nextSessionListRequestId = { current: 41 };

    requestControllerNativeSessions({
      agentId: "codex",
      append: true,
      cursor: "cursor_2",
      dispatch,
      latestSessionListRequestId,
      nextSessionListRequestId,
    });

    expect(nextSessionListRequestId.current).toBe(42);
    expect(latestSessionListRequestId.current).toBe(42);
    expect(dispatch).toHaveBeenCalledWith({ type: "newTask:nativeSessions:start", append: true });
    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:nativeSessions:error",
      message: "App Server connection unavailable.",
    });
  });

  it("defaults append to false for fresh navigation loads", () => {
    const dispatch = vi.fn();
    const latestSessionListRequestId = { current: undefined as number | undefined };
    const nextSessionListRequestId = { current: 0 };

    requestControllerNativeSessions({
      agentId: "codex",
      dispatch,
      latestSessionListRequestId,
      nextSessionListRequestId,
    });

    expect(dispatch).toHaveBeenCalledWith({ type: "newTask:nativeSessions:start", append: false });
    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:nativeSessions:error",
      message: "App Server connection unavailable.",
    });
  });

  it("uses the typed App Server session list when a backend connection is available", async () => {
    const dispatch = vi.fn();
    const postHostMessage = vi.fn();
    const request = vi.fn().mockResolvedValue({
      agentId: "codex",
      projectId: "project-1",
      projectLabel: "Workspace",
      sessions: [{
        sessionId: "session_1",
        title: "Existing session",
        lastActivity: "2026-05-18T00:00:00Z",
        updatedAt: "2026-05-18T00:00:00Z",
      }],
      nextCursor: "cursor_2",
    });
    const latestSessionListRequestId = { current: undefined as number | undefined };
    const nextSessionListRequestId = { current: 0 };

    requestControllerNativeSessions({
      agentId: "codex",
      backendConnection: { request },
      dispatch,
      latestSessionListRequestId,
      nextSessionListRequestId,
      projectId: "project-1",
    });

    await Promise.resolve();

    expect(dispatch).toHaveBeenCalledWith({
      append: false,
      type: "newTask:nativeSessions:result",
      result: {
        agent_id: "codex",
        next_cursor: "cursor_2",
        sessions: [{
          cwd: "Workspace",
          session_id: "session_1",
          title: "Existing session",
          last_activity: "2026-05-18T00:00:00Z",
          updated_at: "2026-05-18T00:00:00Z",
        }],
      },
    });
    expect(postHostMessage).not.toHaveBeenCalled();
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
