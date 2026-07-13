import { describe, expect, it, vi } from "vitest";
import type { TaskSummary } from "@openaide/app-shell-contracts";
import { AppServerProtocolError } from "@openaide/app-server-client";
import { createInitialState } from "../state/store";
import { AsyncOperationOwner } from "../state/asyncOperationOwner";
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

describe("requestControllerNativeSessions", () => {
  it("refreshes enough session-history pages to preserve the expanded task list", async () => {
    const dispatch = vi.fn();
    const request = vi.fn()
      .mockResolvedValueOnce({
        agentId: "codex",
        projectId: "project-1",
        projectLabel: "Workspace",
        sessions: [
          { sessionId: "session_1", title: "Newest" },
          { sessionId: "session_2", title: "Recent" },
        ],
        nextCursor: "cursor_2",
      })
      .mockResolvedValueOnce({
        agentId: "codex",
        projectId: "project-1",
        projectLabel: "Workspace",
        sessions: [
          { sessionId: "session_3", title: "Older" },
          { sessionId: "session_4", title: "Oldest" },
        ],
        nextCursor: "cursor_3",
      });

    requestControllerNativeSessions({
      agentId: "codex",
      asyncOperations: new AsyncOperationOwner(),
      backendConnection: { request },
      dispatch,
      minimumSessionCount: 3,
      projectId: "project-1",
    });
    await vi.waitFor(() => expect(request).toHaveBeenCalledTimes(2));

    expect(request).toHaveBeenNthCalledWith(1, "agent/listSessions", {
      agentId: "codex",
      projectId: "project-1",
      cursor: null,
    });
    expect(request).toHaveBeenNthCalledWith(2, "agent/listSessions", {
      agentId: "codex",
      projectId: "project-1",
      cursor: "cursor_2",
    });
    expect(dispatch).toHaveBeenLastCalledWith({
      type: "newTask:nativeSessions:result",
      append: false,
      result: {
        agent_id: "codex",
        next_cursor: "cursor_3",
        sessions: [
          { session_id: "session_1", cwd: "Workspace", title: "Newest" },
          { session_id: "session_2", cwd: "Workspace", title: "Recent" },
          { session_id: "session_3", cwd: "Workspace", title: "Older" },
          { session_id: "session_4", cwd: "Workspace", title: "Oldest" },
        ],
      },
    });
  });

  it("loads the requested number of new unique sessions when appending pages", async () => {
    const dispatch = vi.fn();
    const request = vi.fn()
      .mockResolvedValueOnce({
        agentId: "codex",
        projectId: "project-1",
        projectLabel: "Workspace",
        sessions: [
          { sessionId: "session_1", title: "Already loaded" },
          { sessionId: "session_2", title: "Newer" },
        ],
        nextCursor: "cursor_3",
      })
      .mockResolvedValueOnce({
        agentId: "codex",
        projectId: "project-1",
        projectLabel: "Workspace",
        sessions: [
          { sessionId: "session_2", title: "Newer duplicate" },
          { sessionId: "session_3", title: "Older" },
        ],
        nextCursor: "cursor_4",
      });

    requestControllerNativeSessions({
      agentId: "codex",
      append: true,
      asyncOperations: new AsyncOperationOwner(),
      backendConnection: { request },
      cursor: "cursor_2",
      dispatch,
      existingSessionIds: ["session_1"],
      minimumSessionCount: 2,
      projectId: "project-1",
    });
    await vi.waitFor(() => expect(request).toHaveBeenCalledTimes(2));

    expect(dispatch).toHaveBeenLastCalledWith({
      type: "newTask:nativeSessions:result",
      append: true,
      result: {
        agent_id: "codex",
        next_cursor: "cursor_4",
        sessions: [
          { session_id: "session_2", cwd: "Workspace", title: "Newer" },
          { session_id: "session_3", cwd: "Workspace", title: "Older" },
        ],
      },
    });
  });

  it("stops pagination when the Agent repeats a session cursor", async () => {
    const dispatch = vi.fn();
    const request = vi.fn()
      .mockResolvedValueOnce({
        agentId: "codex",
        projectId: "project-1",
        projectLabel: "Workspace",
        sessions: [{ sessionId: "session_2", title: "Only new session" }],
        nextCursor: "cursor_2",
      })
      .mockRejectedValue(new Error("repeated cursor must not be requested"));

    requestControllerNativeSessions({
      agentId: "codex",
      append: true,
      asyncOperations: new AsyncOperationOwner(),
      backendConnection: { request },
      cursor: "cursor_2",
      dispatch,
      existingSessionIds: ["session_1"],
      minimumSessionCount: 2,
      projectId: "project-1",
    });
    await vi.waitFor(() => expect(dispatch).toHaveBeenCalledTimes(2));

    expect(request).toHaveBeenCalledTimes(1);
    expect(dispatch).toHaveBeenLastCalledWith({
      type: "newTask:nativeSessions:result",
      append: true,
      result: {
        agent_id: "codex",
        next_cursor: undefined,
        sessions: [{ session_id: "session_2", cwd: "Workspace", title: "Only new session" }],
      },
    });
  });

  it("reports the typed App Server failure when session history cannot load", async () => {
    const dispatch = vi.fn();
    const onFailure = vi.fn();
    const request = vi.fn().mockRejectedValue(new AppServerProtocolError({
      error: {
        code: "notFound",
        message: "Project project-current was not found",
        target: { method: "agent/listSessions", field: "projectId" },
      },
    }));

    requestControllerNativeSessions({
      agentId: "codex",
      asyncOperations: new AsyncOperationOwner(),
      backendConnection: { request },
      dispatch,
      onFailure,
      projectId: "project-current",
    });
    await Promise.resolve();
    await Promise.resolve();

    expect(onFailure).toHaveBeenCalledWith({
      agentId: "codex",
      errorCode: "notFound",
      errorMessage: "Project project-current was not found",
      errorName: "AppServerProtocolError",
      projectId: "project-current",
      request: "agent/listSessions",
      requestId: 1,
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:nativeSessions:listError",
      message: "Unable to load Agent session history.",
    });
  });

  it("reports an error when listing sessions without BackendConnection", () => {
    const dispatch = vi.fn();

    requestControllerNativeSessions({
      agentId: "codex",
      append: true,
      asyncOperations: new AsyncOperationOwner(),
      cursor: "cursor_2",
      dispatch,
    });

    expect(dispatch).toHaveBeenCalledWith({ type: "newTask:nativeSessions:start", append: true });
    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:nativeSessions:listError",
      message: "App Server connection unavailable.",
    });
  });

  it("defaults append to false for fresh navigation loads", () => {
    const dispatch = vi.fn();

    requestControllerNativeSessions({
      agentId: "codex",
      asyncOperations: new AsyncOperationOwner(),
      dispatch,
    });

    expect(dispatch).toHaveBeenCalledWith({ type: "newTask:nativeSessions:start", append: false });
    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:nativeSessions:listError",
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
    requestControllerNativeSessions({
      agentId: "codex",
      asyncOperations: new AsyncOperationOwner(),
      backendConnection: { request },
      dispatch,
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
