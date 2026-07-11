import { describe, expect, it } from "vitest";
import type { ActivityStep, AgentSettingsRecord, ChatMessage, MessagePage, TaskSnapshot, TaskSummary } from "@openaide/app-shell-contracts";
import { appReducer } from "./appReducer";
import { renderedChat } from "./chatPaging";
import { createInitialState } from "./store";

type PermissionChatMessage = ChatMessage & {
  message: Extract<ChatMessage["message"], { kind: "permission" }>;
};

describe("app reducer composer state", () => {
  it("tracks custom Agent save/delete acknowledgements and clears them on refresh", () => {
    let state = createInitialState();

    state = appReducer(state, { type: "settings:agentSaved", agentId: "custom.agent" });
    expect(state.settings.savedAgentId).toBe("custom.agent");
    expect(state.settings.deletedAgentId).toBeUndefined();

    state = appReducer(state, { type: "settings:agentDeleted", agentId: "custom.agent" });
    expect(state.settings.deletedAgentId).toBe("custom.agent");
    expect(state.settings.savedAgentId).toBeUndefined();

    state = appReducer(state, { type: "settings:start" });
    expect(state.settings.savedAgentId).toBeUndefined();
    expect(state.settings.deletedAgentId).toBeUndefined();
  });

  it("stores Backend Agent Settings details without fabricating a full Settings snapshot", () => {
    let state = createInitialState();
    const agents = settingsAgents(["codex", "custom.agent"]);

    state = appReducer(state, {
      type: "settings:agentDetailsResult",
      generatedAt: "now",
      agents,
    });

    expect(state.settings.loading).toBe(false);
    expect(state.settings.error).toBeUndefined();
    expect(state.settings.agentDetails?.map((agent) => agent.id)).toEqual(["codex", "custom.agent"]);
  });

  it("reconciles Agent mutations into Backend Agent Settings details", () => {
    let state = createInitialState();
    const codex = settingsAgents(["codex"])[0];
    const custom = settingsAgents(["custom.agent"])[0];

    state = appReducer(state, {
      type: "settings:agentDetailsResult",
      generatedAt: "now",
      agents: [codex],
    });
    state = appReducer(state, {
      type: "settings:agentSaved",
      agentId: "custom.agent",
      agent: custom,
    });
    expect(state.settings.agentDetails?.map((agent) => agent.id)).toEqual(["codex", "custom.agent"]);

    state = appReducer(state, {
      type: "settings:agentReplaced",
      oldAgentId: "custom.agent",
      newAgentId: "custom.replacement",
      agent: { ...custom, id: "custom.replacement" },
    });
    expect(state.settings.agentDetails?.map((agent) => agent.id)).toEqual(["codex", "custom.replacement"]);
    expect(state.settings.savedAgentId).toBe("custom.replacement");
    expect(state.settings.deletedAgentId).toBe("custom.agent");

    state = appReducer(state, {
      type: "settings:agentUpdated",
      agent: { ...custom, id: "custom.replacement", enabled: false, status: "disabled" },
    });
    expect(state.settings.agentDetails?.find((agent) => agent.id === "custom.replacement")).toMatchObject({
      enabled: false,
      status: "disabled",
    });

    state = appReducer(state, { type: "settings:agentDeleted", agentId: "custom.replacement" });
    expect(state.settings.agentDetails?.map((agent) => agent.id)).toEqual(["codex"]);
  });

  it("keeps new-task composer selections and context as local state", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "projects",
      activeProjectId: "project-2",
      projects: [
        { projectId: "project-1", label: "API" },
        { projectId: "project-2", label: "App" },
      ],
    });
    state = appReducer(state, {
      type: "workspace:roots",
      roots: [{ path: "/workspace/app", label: "App", projectId: "project-2" }],
    });
    state = appReducer(state, { type: "newTask:agent", agentId: "codex" });
    state = appReducer(state, {
      type: "newTask:configOptions:result",
      catalog: {
        agent_id: "codex",
        status: "ready",
        options: [
          {
            id: "model",
            label: "Model",
            category: "model",
            current_value: "gpt-5.5",
            values: [{ id: "gpt-5.5", label: "gpt-5.5" }],
          },
        ],
      },
    });
    state = appReducer(state, {
      type: "newTask:attachment:add",
      attachment: { kind: "context", label: "App", path: "/workspace/app" },
    });

    expect(state.newTask.selection.agentId).toBe("codex");
    expect(state.newTask.selection.agentLabel).toBe("Codex");
    expect(state.newTask.selection.configOptions.model).toBe("gpt-5.5");
    expect(state.newTask.selection.projectId).toBe("project-2");
    expect(state.newTask.selection.workspaceRoot).toBe("/workspace/app");
    expect(state.newTask.context[0]).toMatchObject({ kind: "context", label: "App", path: "/workspace/app" });
  });

  it("selects a new-task project by id and refreshes its label from project snapshots", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "projects",
      activeProjectId: "project-1",
      projects: [
        { projectId: "project-1", label: "API" },
        { projectId: "project-2", label: "App" },
      ],
    });

    state = appReducer(state, { type: "newTask:projectId", projectId: "project-2" });

    expect(state.newTask.selection.projectId).toBe("project-2");
    expect(state.newTask.selection.workspaceLabel).toBe("App");

    state = appReducer(state, {
      type: "projects",
      activeProjectId: "project-1",
      projects: [
        { projectId: "project-1", label: "API" },
        { projectId: "project-2", label: "Renamed App" },
      ],
    });

    expect(state.newTask.selection.projectId).toBe("project-2");
    expect(state.newTask.selection.workspaceLabel).toBe("Renamed App");
  });

  it("keeps loaded new-task Agent options when the same project id is selected again", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "projects",
      activeProjectId: "project-1",
      projects: [{ projectId: "project-1", label: "OpenAIDE" }],
    });
    state = appReducer(state, { type: "newTask:projectId", projectId: "project-1" });
    state = appReducer(state, {
      type: "newTask:configOptions:result",
      catalog: {
        agent_id: "codex",
        status: "ready",
        options: [
          {
            id: "model",
            label: "Model",
            category: "model",
            current_value: "gpt-5.5",
            values: [{ id: "gpt-5.5", label: "gpt-5.5" }],
          },
        ],
      },
    });

    state = appReducer(state, { type: "newTask:projectId", projectId: "project-1" });

    expect(state.newTask.configOptions?.options).toHaveLength(1);
    expect(state.newTask.configOptionsLoading).toBe(false);
    expect(state.newTask.selection.configOptions).toEqual({ model: "gpt-5.5" });
  });

  it("marks workspace roots as loaded even when no root exists", () => {
    let state = createInitialState();

    state = appReducer(state, { type: "workspace:roots", roots: [] });

    expect(state.workspaceRootsLoaded).toBe(true);
    expect(state.newTask.selection.workspaceRoot).toBe("");
  });

  it("replaces config option selections from the latest complete catalog", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "newTask:configOptions:result",
      catalog: {
        agent_id: "codex",
        status: "ready",
        options: [
          {
            id: "model",
            label: "Model",
            category: "model",
            current_value: "gpt-5.5",
            values: [{ id: "gpt-5.5", label: "gpt-5.5" }],
          },
          {
            id: "reasoning",
            label: "Reasoning",
            category: "thought_level",
            current_value: "medium",
            values: [{ id: "medium", label: "Medium" }],
          },
        ],
      },
    });

    state = appReducer(state, {
      type: "newTask:configOptions:result",
      catalog: {
        agent_id: "codex",
        status: "ready",
        options: [
          {
            id: "model",
            label: "Model",
            category: "model",
            current_value: "gpt-5.4",
            values: [{ id: "gpt-5.4", label: "gpt-5.4" }],
          },
          {
            id: "mode",
            label: "Mode",
            category: "mode",
            current_value: "code",
            values: [{ id: "code", label: "Code" }],
          },
        ],
      },
    });

    expect(state.newTask.selection.configOptions).toEqual({
      model: "gpt-5.4",
      mode: "code",
    });
  });

  it("clears prepared config options when the workspace selector changes", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "newTask:configOptions:result",
      catalog: {
        agent_id: "codex",
        status: "ready",
        options: [
          {
            id: "model",
            label: "Model",
            category: "model",
            current_value: "gpt-5.5",
            values: [{ id: "gpt-5.5", label: "gpt-5.5" }],
          },
        ],
      },
    });

    state = appReducer(state, {
      type: "newTask:workspace",
      workspace: { path: "/workspace/other", label: "Other" },
    });

    expect(state.newTask.configOptions).toBeUndefined();
    expect(state.newTask.selection.configOptions).toEqual({});
    expect(state.newTask.selection.workspaceRoot).toBe("/workspace/other");
    expect(state.newTask.nativeSessions.items).toEqual([]);
    expect(state.newTask.nativeSessions.loaded).toBe(false);
  });

  it("stores previous native sessions and merges loaded pages", () => {
    let state = createInitialState();

    state = appReducer(state, { type: "newTask:nativeSessions:start", append: false });
    expect(state.newTask.nativeSessions.loading).toBe(true);

    state = appReducer(state, {
      type: "newTask:nativeSessions:result",
      append: false,
      result: {
        agent_id: "codex",
        sessions: [{ session_id: "session_1", cwd: "/workspace/app", title: "First" }],
        next_cursor: "cursor_2",
      },
    });
    state = appReducer(state, {
      type: "newTask:nativeSessions:result",
      append: true,
      result: {
        agent_id: "codex",
        sessions: [
          { session_id: "session_1", cwd: "/workspace/app", title: "First updated" },
          { session_id: "session_2", cwd: "/workspace/app", title: "Second" },
        ],
      },
    });

    expect(state.newTask.nativeSessions.loading).toBe(false);
    expect(state.newTask.nativeSessions.items.map((session) => session.title)).toEqual([
      "First updated",
      "Second",
    ]);
    expect(state.newTask.nativeSessions.nextCursor).toBeUndefined();
  });

  it("tracks native session adoption and clears it on errors", () => {
    let state = createInitialState();

    state = appReducer(state, { type: "newTask:nativeSessions:adopt", sessionId: "session_1" });
    expect(state.newTask.submitting).toBe(true);
    expect(state.newTask.nativeSessions.adoptingSessionId).toBe("session_1");

    state = appReducer(state, { type: "submit:error", message: "Already adopted" });
    expect(state.newTask.submitting).toBe(false);
    expect(state.newTask.nativeSessions.adoptingSessionId).toBeUndefined();
    expect(state.newTask.error).toBe("Already adopted");

    state = appReducer(state, { type: "newTask:nativeSessions:adopt", sessionId: "session_2" });
    state = appReducer(state, { type: "newTask:nativeSessions:error", message: "Unable to open task." });
    expect(state.newTask.submitting).toBe(false);
    expect(state.newTask.nativeSessions.adoptingSessionId).toBeUndefined();
    expect(state.newTask.nativeSessions.error).toBe("Unable to open task.");

    state = appReducer(state, {
      type: "newTask:nativeSessions:result",
      append: false,
      result: {
        agent_id: "codex",
        sessions: [
          { session_id: "session_2", cwd: "/workspace/app", title: "Second" },
          { session_id: "session_3", cwd: "/workspace/app", title: "Third" },
        ],
      },
    });
    state = appReducer(state, { type: "newTask:nativeSessions:adopt", sessionId: "session_2" });
    state = appReducer(state, { type: "newTask:nativeSessions:remove", sessionId: "session_2" });
    expect(state.newTask.submitting).toBe(false);
    expect(state.newTask.nativeSessions.adoptingSessionId).toBeUndefined();
    expect(state.newTask.nativeSessions.items.map((session) => session.session_id)).toEqual(["session_3"]);
  });

  it("clears previous native sessions when the agent selector changes", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "newTask:nativeSessions:result",
      append: false,
      result: {
        agent_id: "codex",
        sessions: [{ session_id: "session_1", cwd: "/workspace/app", title: "Existing" }],
      },
    });

    state = appReducer(state, { type: "newTask:agent", agentId: "mock" });

    expect(state.newTask.nativeSessions.items).toEqual([]);
    expect(state.newTask.nativeSessions.loaded).toBe(false);
    expect(state.newTask.nativeSessions.loading).toBe(false);
  });

  it("tracks App Server connection errors", () => {
    let state = createInitialState();

    state = appReducer(state, { type: "appServer:error", message: "App Server unavailable" });
    expect(state.appServerError).toBe("App Server unavailable");
    expect(state.taskListError).toBe("App Server unavailable");

    state = appReducer(state, { type: "appServer:ready" });
    expect(state.appServerError).toBeUndefined();
  });

  it("clears submitted follow-up input and restores it on task send failure", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Use context" });
    state = appReducer(state, {
      type: "taskInput:attachment:add",
      taskId: "task_1",
      attachment: { kind: "file", label: "src/main.rs", path: "/workspace/src/main.rs" },
    });

    state = appReducer(state, { type: "taskInput:submit", taskId: "task_1" });
    expect(state.taskInputs.task_1.prompt).toBe("");
    expect(state.taskInputs.task_1.context).toHaveLength(0);
    expect(state.taskInputs.task_1.pending?.prompt).toBe("Use context");

    state = appReducer(state, { type: "taskInput:error", taskId: "task_1", message: "Send failed" });
    expect(state.taskInputs.task_1.prompt).toBe("Use context");
    expect(state.taskInputs.task_1.context[0]).toMatchObject({ label: "src/main.rs" });
    expect(state.taskInputs.task_1.error).toBe("Send failed");
    expect(state.taskInputs.task_1.pending).toBeUndefined();
  });

  it("clears local follow-up drafts for a task", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Do not send" });

    state = appReducer(state, { type: "taskInput:clear", taskId: "task_1" });

    expect(state.taskInputs.task_1).toBeUndefined();
  });

  it("drops pending follow-up input after the accepted task snapshot", () => {
    let state = createInitialState();
    state = { ...state, activeTaskId: "task_1" };
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Accepted" });
    state = appReducer(state, { type: "taskInput:submit", taskId: "task_1" });

    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: snapshot("task_1", [userMessage("user_1", "Accepted")]),
    });

    expect(state.taskInputs.task_1.prompt).toBe("");
    expect(state.taskInputs.task_1.context).toHaveLength(0);
    expect(state.taskInputs.task_1.pending).toBeUndefined();
  });

  it("clears a restored send error draft after a later snapshot proves it committed", () => {
    let state = createInitialState();
    state = { ...state, activeTaskId: "task_1" };
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Stop now" });
    state = appReducer(state, { type: "taskInput:submit", taskId: "task_1" });
    state = appReducer(state, { type: "taskInput:error", taskId: "task_1", message: "request failed" });

    expect(state.taskInputs.task_1.prompt).toBe("Stop now");
    expect(state.taskInputs.task_1.error).toBe("request failed");

    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: snapshot("task_1", [userMessage("user_1", "Stop now")], 2),
    });

    expect(state.taskInputs.task_1.prompt).toBe("");
    expect(state.taskInputs.task_1.context).toHaveLength(0);
    expect(state.taskInputs.task_1.pending).toBeUndefined();
    expect(state.taskInputs.task_1.error).toBeUndefined();
  });

  it("keeps submitted follow-up visible as pending until committed chat arrives", () => {
    let state = createInitialState();
    state = { ...state, activeTaskId: "task_1" };
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Still pending" });
    state = appReducer(state, { type: "taskInput:submit", taskId: "task_1" });

    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: {
        ...snapshot("task_1", []),
        task: { ...taskSummary("task_1"), has_messages: true },
        chat: { ...snapshot("task_1", []).chat, has_messages: true, total_count: 1 },
      },
    });

    expect(state.taskInputs.task_1.pending?.prompt).toBe("Still pending");
  });

  it("keeps accepted send chat visible when an older refresh snapshot arrives late", () => {
    let state = createInitialState();
    state = { ...state, activeTaskId: "task_1" };
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Accepted" });
    state = appReducer(state, { type: "taskInput:submit", taskId: "task_1" });

    const accepted = snapshot("task_1", [userMessage("user_1", "Accepted")], 2);
    state = appReducer(state, { type: "snapshot", intent: "refresh", snapshot: accepted });
    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: snapshot("task_1", [], 1),
    });

    expect(state.snapshot?.revision).toBe(2);
    expect(state.snapshot?.chat.items.map((item) => item.message_id)).toEqual(["user_1"]);
    expect(state.taskInputs.task_1.pending).toBeUndefined();
  });

  it("does not replace visible chat with a same-revision incomplete refresh", () => {
    let state = createInitialState();
    state = { ...state, activeTaskId: "task_1" };
    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: snapshot("task_1", [userMessage("user_1", "Visible")], 2),
    });

    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: snapshot("task_1", [], 2),
    });

    expect(state.snapshot?.chat.items.map((item) => item.message_id)).toEqual(["user_1"]);
  });

  it("clears pending input from an incomplete refresh that proves the submitted message committed", () => {
    let state = createInitialState();
    state = { ...state, activeTaskId: "task_1" };
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Committed" });
    state = appReducer(state, { type: "taskInput:submit", taskId: "task_1" });
    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: snapshot("task_1", [
        userMessage("user_1", "Committed"),
        chatMessage("agent_1", "working"),
      ], 2),
    });
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Committed again" });
    state = appReducer(state, { type: "taskInput:submit", taskId: "task_1" });

    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: snapshot("task_1", [userMessage("user_2", "Committed again")], 2),
    });

    expect(state.snapshot?.chat.items.map((item) => item.message_id)).toEqual(["user_1", "agent_1"]);
    expect(state.taskInputs.task_1.pending).toBeUndefined();
  });

  it("keeps pending follow-up input through no-message preparation snapshots", () => {
    let state = createInitialState();
    state = { ...state, activeTaskId: "task_1" };
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Pending send" });
    state = appReducer(state, { type: "taskInput:submit", taskId: "task_1" });

    state = appReducer(state, { type: "snapshot", intent: "refresh", snapshot: noMessageSnapshot("task_1") });

    expect(state.taskInputs.task_1.prompt).toBe("");
    expect(state.taskInputs.task_1.context).toHaveLength(0);
    expect(state.taskInputs.task_1.pending?.prompt).toBe("Pending send");
  });

  it("keeps new-task startup submitting through prepared snapshots before first message commits", () => {
    let state = createInitialState();
    state.newTask.prompt = "Build the thing";
    state.newTask.configOptions = {
      agent_id: "codex",
      options: [{
        category: "model",
        current_value: "gpt-5.5",
        id: "model",
        label: "Model",
        values: [{ id: "gpt-5.5", label: "GPT-5.5" }],
      }],
      status: "ready",
    };
    state = appReducer(state, { type: "submit:start" });

    expect(state.newTask.prompt).toBe("");
    expect(state.newTask.pending?.prompt).toBe("Build the thing");
    expect(state.newTask.pending?.configOptions?.options[0].current_value).toBe("gpt-5.5");

    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: noMessageSnapshot("task_new"),
    });

    expect(state.newTask.submitting).toBe(true);
  });

  it("restores the new-task draft when startup submission fails", () => {
    let state = createInitialState();
    state.newTask.prompt = "Build the thing";

    state = appReducer(state, { type: "submit:start" });
    state = appReducer(state, { type: "submit:error", message: "Send failed" });

    expect(state.newTask.submitting).toBe(false);
    expect(state.newTask.prompt).toBe("Build the thing");
    expect(state.newTask.pending).toBeUndefined();
    expect(state.newTask.error).toBe("Send failed");
  });

  it("restores the submitted draft without an error when startup is stopped", () => {
    let state = createInitialState();
    state.newTask.prompt = "Build the thing";

    state = appReducer(state, { type: "submit:start" });
    state = appReducer(state, { type: "submit:cancel" });

    expect(state.newTask.submitting).toBe(false);
    expect(state.newTask.prompt).toBe("Build the thing");
    expect(state.newTask.pending).toBeUndefined();
    expect(state.newTask.error).toBeUndefined();
  });

  it("adds an opened task snapshot to the task list so the sidebar can show it", () => {
    let state = createInitialState();

    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_1") });

    expect(state.tasks.map((task) => task.task_id)).toEqual(["task_1"]);
  });

  it("updates an existing task list row from task snapshots without reordering it", () => {
    let state = createInitialState();
    state = {
      ...state,
      tasks: [
        { ...taskSummary("task_1"), title: "Old title" },
        taskSummary("task_2"),
      ],
    };

    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: { ...snapshot("task_1"), task: { ...taskSummary("task_1"), title: "New title" } },
    });

    expect(state.tasks.map((task) => task.task_id)).toEqual(["task_1", "task_2"]);
    expect(state.tasks[0].title).toBe("New title");
  });

  it("keeps a locally pending task when navigation refresh omits empty tasks", () => {
    let state = createInitialState();
    state.tasks = [
      taskSummary("task_1"),
      { ...taskSummary("task_new"), has_messages: false, title: "New task" },
    ];
    state.taskInputs.task_new = {
      prompt: "",
      context: [],
      pending: { prompt: "Ship it", context: [] },
    };

    state = appReducer(state, { type: "tasks", tasks: [taskSummary("task_1")] });

    expect(state.tasks.map((task) => task.task_id)).toEqual(["task_1", "task_new"]);
    expect(state.taskListCache.active?.map((task) => task.task_id)).toEqual(["task_1", "task_new"]);
  });

  it("reuses a cached task snapshot immediately when selecting a previously opened task", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: { ...snapshot("task_1"), task: { ...taskSummary("task_1"), title: "First cached" } },
    });
    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: { ...snapshot("task_2"), task: { ...taskSummary("task_2"), title: "Second active" } },
    });

    state = appReducer(state, { type: "selection:set", taskId: "task_1" });

    expect(state.activeTaskId).toBe("task_1");
    expect(state.snapshot?.task.title).toBe("First cached");
  });

  it("lets task navigation leave a slow native-session opening state", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_1") });
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_2") });
    state = appReducer(state, { type: "newTask:nativeSessions:adopt", sessionId: "native_1" });

    state = appReducer(state, { type: "selection:set", taskId: "task_1" });

    expect(state.activeTaskId).toBe("task_1");
    expect(state.snapshot?.task.task_id).toBe("task_1");
    expect(state.newTask.submitting).toBe(false);
    expect(state.newTask.nativeSessions.adoptingSessionId).toBeUndefined();
  });

  it("removes archived or restored tasks from the current navigation slice", () => {
    let state = createInitialState();
    state = {
      ...state,
      activeTaskId: "task_1",
      snapshot: snapshot("task_1"),
      tasks: [taskSummary("task_1"), taskSummary("task_2")],
    };

    state = appReducer(state, { type: "task:list:remove", taskId: "task_1" });

    expect(state.tasks.map((task) => task.task_id)).toEqual(["task_2"]);
    expect(state.activeTaskId).toBeUndefined();
    expect(state.snapshot).toBeUndefined();
  });

  it("stores task-open errors until a snapshot is accepted", () => {
    let state = createInitialState();

    state = appReducer(state, { type: "taskOpen:error", taskId: "task_1", message: "Unable to open task" });
    expect(state.taskOpenError).toEqual({ taskId: "task_1", message: "Unable to open task" });

    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_1") });
    expect(state.taskOpenError).toBeUndefined();
  });

  it("records chat scroll position per task", () => {
    let state = createInitialState();

    state = appReducer(state, { type: "taskScroll:record", taskId: "task_1", scrollTop: 320 });
    state = appReducer(state, { type: "taskScroll:record", taskId: "task_2", scrollTop: 80 });

    expect(state.taskScrollPositions).toEqual({
      task_1: 320,
      task_2: 80,
    });
  });

  it("keeps the current navigation context when opening a task snapshot", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "archive:set", showArchived: true });
    state = appReducer(state, { type: "search:set", query: "no-match-navigation-qa-20260630" });

    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_1") });

    expect(state.showArchived).toBe(true);
    expect(state.searchQuery).toBe("");
    expect(state.tasks.map((task) => task.task_id)).toEqual(["task_1"]);
  });

  it("keeps the open task page while switching the sidebar archive filter", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_1") });

    state = appReducer(state, { type: "archive:set", showArchived: true });

    expect(state.showArchived).toBe(true);
    expect(state.tasks).toEqual([]);
    expect(state.activeTaskId).toBe("task_1");
    expect(state.snapshot?.task.task_id).toBe("task_1");
  });

  it("restores cached task lists when switching archive filters", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "tasks", tasks: [taskSummary("task_active")] });
    state = appReducer(state, { type: "archive:set", showArchived: true });
    state = appReducer(state, { type: "tasks", tasks: [taskSummary("task_archived")] });

    state = appReducer(state, { type: "archive:set", showArchived: false });
    expect(state.tasks.map((task) => task.task_id)).toEqual(["task_active"]);

    state = appReducer(state, { type: "archive:set", showArchived: true });
    expect(state.tasks.map((task) => task.task_id)).toEqual(["task_archived"]);
  });

  it("stores task-list errors until a list or archive-mode change is accepted", () => {
    let state = createInitialState();

    state = appReducer(state, { type: "tasks:error", message: "Unable to load tasks" });
    expect(state.taskListError).toBe("Unable to load tasks");

    state = appReducer(state, { type: "tasks", tasks: [taskSummary("task_1")] });
    expect(state.taskListError).toBeUndefined();

    state = appReducer(state, { type: "tasks:error", message: "Unable to load archive" });
    state = appReducer(state, { type: "archive:set", showArchived: true });
    expect(state.taskListError).toBeUndefined();
  });

  it("prepends earlier chat pages without duplicating already loaded message rows", () => {
    let state = createInitialState();
    const taskSnapshot = snapshot("task_1", [
      chatMessage("m3", "tail 3"),
      chatMessage("m4", "tail 4"),
    ]);
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: taskSnapshot });
    state = appReducer(state, { type: "chatPage:start", taskId: "task_1" });
    state = appReducer(state, {
      type: "chatPage:result",
      taskId: "task_1",
      page: page("task_1", [chatMessage("m1", "older 1"), chatMessage("m3", "tail 3")], false),
    });

    const chat = renderedChat(taskSnapshot, state.chatPages.task_1);

    expect(chat.items.map((message) => message.message_id)).toEqual(["m1", "m3", "m4"]);
    expect(chat.hasBefore).toBe(false);
    expect(state.chatPages.task_1.pending).toBe(false);
  });

  it("coalesces adjacent persisted agent text chunks for rendering", () => {
    const taskSnapshot = snapshot("task_1", [
      chatMessage("m1", "Called"),
      chatMessage("m2", " `"),
      chatMessage("m3", "pwd"),
      chatMessage("m4", "`:"),
      chatMessage("m5", " `/"),
      chatMessage("m6", "home"),
      chatMessage("m7", "/us"),
      chatMessage("m8", "er"),
      chatMessage("m9", "`"),
    ]);

    const chat = renderedChat(taskSnapshot, undefined);

    expect(chat.items).toHaveLength(1);
    expect(chat.items[0].message).toMatchObject({
      kind: "agent_text",
      text: "Called `pwd`: `/home/user`",
    });
  });

  it("omits working boilerplate from rendered chat", () => {
    const taskSnapshot = snapshot("task_1", [
      activityMessage("m0", "Working", "running", [{ kind: "text", text: "Started", level: "info" }]),
      activityMessage("m1", "Working", "completed", [{ kind: "text", text: "Started", level: "info" }]),
      activityMessage("m2", "Working", "running", [{ kind: "text", text: "Working", level: "info" }]),
      chatMessage("m3", "Done"),
    ]);

    const chat = renderedChat(taskSnapshot, undefined);

    expect(chat.items.map((message) => message.message_id)).toEqual(["m3"]);
  });

  it("omits legacy session catalog activity rows from rendered chat", () => {
    const taskSnapshot = snapshot("task_1", [
      activityMessage("m1", "Updated slash commands", "completed", [
        { kind: "text", text: "Slash commands changed.", level: "info" },
      ]),
      activityMessage("m2", "Updated session options", "completed", [
        { kind: "text", text: "Session options changed.", level: "info" },
      ]),
      chatMessage("m3", "Done"),
    ]);

    const chat = renderedChat(taskSnapshot, undefined);

    expect(chat.items.map((message) => message.message_id)).toEqual(["m3"]);
  });

  it("converts legacy streamed thought activity rows into thought chat", () => {
    const taskSnapshot = snapshot("task_1", [
      activityMessage("m1", "Thought", "completed", [
        { kind: "tool", name: "think", status: "completed", output_preview: "The" },
      ]),
      activityMessage("m2", "Thought", "completed", [
        { kind: "tool", name: "think", status: "completed", output_preview: " user" },
      ]),
      chatMessage("m3", "Done"),
    ]);

    const chat = renderedChat(taskSnapshot, undefined);

    expect(chat.items).toHaveLength(2);
    expect(chat.items[0].message).toMatchObject({
      kind: "thought",
      text: "The user",
    });
    expect(chat.items[1].message_id).toBe("m3");
  });

  it("groups adjacent thought rows with tool activity", () => {
    const taskSnapshot = snapshot("task_1", [
      thoughtMessage("m1", "The"),
      thoughtMessage("m2", " user asked for search"),
      activityMessage("m3", "Search files", "completed", [
        { kind: "tool", name: "search", status: "completed", input_summary: "beta" },
      ]),
    ]);

    const chat = renderedChat(taskSnapshot, undefined);

    expect(chat.items).toHaveLength(1);
    expect(chat.items[0].message).toMatchObject({
      kind: "activity",
      title: "Tool activity",
      steps: [
        { kind: "thought", text: "The user asked for search" },
        { kind: "tool", name: "search", input_summary: "beta" },
      ],
    });
  });

  it("groups adjacent activity rows for rendering", () => {
    const taskSnapshot = snapshot("task_1", [
      activityMessage("m1", "exec_command", "completed", [
        { kind: "tool", name: "execute", status: "completed", input_summary: "git status --short" },
      ]),
      activityMessage("m2", "exec_command", "completed", [
        { kind: "tool", name: "execute", status: "completed", input_summary: "npm run check" },
      ]),
      chatMessage("m3", "Done"),
      activityMessage("m4", "Search files", "completed", [
        { kind: "tool", name: "search", status: "completed", input_summary: "activity" },
      ]),
    ]);

    const chat = renderedChat(taskSnapshot, undefined);

    expect(chat.items).toHaveLength(3);
    expect(chat.items[0].message).toMatchObject({
      kind: "activity",
      title: "Commands",
      status: "completed",
      steps: [
        { kind: "tool", input_summary: "git status --short" },
        { kind: "tool", input_summary: "npm run check" },
      ],
    });
    expect(chat.items[1].message_id).toBe("m3");
    expect(chat.items[2].message).toMatchObject({ kind: "activity", title: "Search files" });
  });

  it("ignores stale earlier pages for a task that is no longer selected", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_current") });

    state = appReducer(state, {
      type: "chatPage:result",
      taskId: "task_old",
      page: page("task_old", [chatMessage("old", "stale")], false),
    });

    expect(state.chatPages.task_old).toBeUndefined();
  });

  it("ignores stale earlier-page errors for a task that is no longer selected", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_current") });

    state = appReducer(state, {
      type: "chatPage:error",
      taskId: "task_old",
      message: "Unable to load earlier messages",
    });

    expect(state.chatPages.task_old).toBeUndefined();
  });

  it("stores settings loading, errors, and selected tab", () => {
    let state = createInitialState();

    state = appReducer(state, { type: "settings:sections", tabs: ["agents", "mcp", "skills", "common"] });
    expect(state.settings.availableTabs).toEqual(["agents", "mcp", "skills", "common"]);

    state = appReducer(state, { type: "settings:start" });
    expect(state.settings.loading).toBe(true);

    state = appReducer(state, { type: "settings:preferences", preferences: { composer_submit_shortcut: "enter" } });
    expect(state.settings.error).toBeUndefined();

    state = appReducer(state, { type: "settings:tab", tab: "skills" });
    expect(state.settings.activeTab).toBe("skills");

    state = appReducer(state, { type: "settings:error", message: "Unable to load settings" });
    expect(state.settings.loading).toBe(false);
    expect(state.settings.error).toBe("Unable to load settings");
  });

  it("stores non-Agent Settings projection loading, results, and errors independently", () => {
    let state = createInitialState();

    state = appReducer(state, { type: "settings:mcpServersStart" });
    expect(state.settings.mcpServersLoading).toBe(true);

    state = appReducer(state, {
      type: "settings:mcpServersResult",
      generatedAt: "mcp-now",
      availability: "available",
      servers: [{ id: "server-1", label: "Filesystem", enabled: true, scope: "global", transport: "stdio", status: "available" }],
    });
    expect(state.settings.mcpServersLoading).toBe(false);
    expect(state.settings.mcpServersAvailability).toBe("available");
    expect(state.settings.mcpServers?.[0].label).toBe("Filesystem");

    state = appReducer(state, { type: "settings:skillsStart" });
    state = appReducer(state, {
      type: "settings:skillsResult",
      generatedAt: "skills-now",
      availability: "unavailable",
      skills: [],
    });
    expect(state.settings.skillsAvailability).toBe("unavailable");

    state = appReducer(state, { type: "settings:skillsStart" });
    state = appReducer(state, { type: "settings:skillsError", message: "Scan failed" });
    expect(state.settings.skillsLoading).toBe(false);
    expect(state.settings.skillsError).toBe("Scan failed");
    expect(state.settings.mcpServers?.[0].id).toBe("server-1");
  });

  it("patches developer runtime settings only after the runtime projection exists", () => {
    let state = createInitialState();

    state = appReducer(state, { type: "settings:developerAcpTrace", enabled: true });
    expect(state.settings.runtimeSettings).toBeUndefined();

    state = appReducer(state, {
      type: "settings:runtimeSettings",
      settings: {
        developer: { acp_trace: { enabled: false, directory: "/runtime/traces" } },
      },
    });

    expect(state.settings.runtimeSettings?.developer.acp_trace).toEqual({
      enabled: false,
      directory: "/runtime/traces",
    });

    state = appReducer(state, { type: "settings:developerAcpTrace", enabled: true });
    expect(state.settings.runtimeSettings?.developer.acp_trace.enabled).toBe(true);
  });

  it("stores backend runtime settings without requiring a full settings snapshot", () => {
    const state = appReducer(createInitialState(), {
      type: "settings:runtimeSettings",
      settings: {
        developer: { acp_trace: { enabled: true, directory: "/runtime/traces" } },
      },
    });

    expect(state.settings.runtimeSettings?.developer.acp_trace).toEqual({
      enabled: true,
      directory: "/runtime/traces",
    });
  });

  it("keeps acknowledged App Server permissions until the chat snapshot resolves them", () => {
    let state = createInitialState();
    const permission = permissionMessage("server-request-1");
    state = appReducer(state, {
      type: "appServerPermission:received",
      requestId: "server-request-1",
      taskId: "task_1",
      message: permission,
    });
    state = appReducer(state, { type: "permission:responding", requestId: "server-request-1" });

    state = appReducer(state, {
      type: "appServerPermission:resolved",
      requestId: "server-request-1",
    });

    expect(state.appServerPermissionRequests["server-request-1"]).toBeDefined();
    expect(state.permissionResponses["server-request-1"]).toEqual({ responding: true });

    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: snapshot("task_1", [permissionMessage("server-request-1")]),
    });

    expect(state.appServerPermissionRequests["server-request-1"]).toBeDefined();
    expect(state.permissionResponses["server-request-1"]).toEqual({ responding: true });
  });

  it("removes App Server permission response state after a durable terminal permission snapshot", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "appServerPermission:received",
      requestId: "server-request-1",
      taskId: "task_1",
      message: permissionMessage("server-request-1"),
    });
    state = appReducer(state, { type: "permission:responding", requestId: "server-request-1" });

    const resolvedPermission = permissionMessage("server-request-1");
    resolvedPermission.message = {
      ...resolvedPermission.message,
      state: "resolved",
      decision: "approved",
      selected_option: "allow_once",
    };
    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: snapshot("task_1", [resolvedPermission]),
    });

    expect(state.appServerPermissionRequests["server-request-1"]).toBeUndefined();
    expect(state.permissionResponses["server-request-1"]).toBeUndefined();
  });
});

function snapshot(taskId: string, items: ChatMessage[] = [], revision = 1): TaskSnapshot {
  const task = taskSummary(taskId);
  return {
    task: { ...task, task_version: revision, message_history_version: revision },
    chat: {
      task_id: taskId,
      items,
      has_before: items.length > 0,
      has_messages: items.length > 0,
      total_count: items.length,
      version: revision,
      start_cursor: items[0]?.cursor,
      end_cursor: items.at(-1)?.cursor,
    },
    permissions: [],
    send_capability: { state: "ready", attachment_only: true },
    settings_summary: {
      agent_id: "codex",
      isolation: "local",
    },
    revision,
  };
}

function noMessageSnapshot(taskId: string): TaskSnapshot {
  const task = { ...taskSummary(taskId), has_messages: false };
  return {
    ...snapshot(taskId, []),
    task,
    chat: {
      task_id: taskId,
      items: [],
      has_before: false,
      has_messages: false,
      total_count: 0,
      version: 1,
    },
  };
}

function taskSummary(taskId: string): TaskSummary {
  const now = "2026-05-17T00:00:00Z";
  return {
    task_id: taskId,
    title: "Task",
    status: "inactive",
    task_version: 1,
    message_history_version: 1,
    has_messages: true,
    unread: false,
    created_at: now,
    updated_at: now,
    last_activity: now,
    agent_id: "codex",
    agent_name: "Codex",
    isolation: "local",
    workspace_root: "/workspace",
  };
}

function page(taskId: string, items: ChatMessage[], hasBefore: boolean): MessagePage {
  return {
    task_id: taskId,
    items,
    has_before: hasBefore,
    has_messages: items.length > 0,
    total_count: items.length,
    version: 1,
    start_cursor: items[0]?.cursor,
    end_cursor: items.at(-1)?.cursor,
  };
}

function chatMessage(id: string, text: string): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "agent_text",
    message_id: id,
    message: {
      kind: "agent_text",
      id,
      text,
      created_at: "2026-05-17T00:00:00Z",
    },
  };
}

function userMessage(id: string, text: string): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "user",
    message_id: id,
    message: {
      kind: "user",
      id,
      text,
      created_at: "2026-05-17T00:00:00Z",
      attachments: [],
    },
  };
}

function permissionMessage(requestId: string): PermissionChatMessage {
  return {
    cursor: requestId,
    identity: requestId,
    message_type: "permission",
    message_id: `app-server-permission-${requestId}`,
    message: {
      kind: "permission",
      id: `app-server-permission-${requestId}`,
      request_id: requestId,
      app_server_request_id: requestId,
      title: "Allow command?",
      tool_call: {
        id: "tool-1",
        title: "Command",
        kind: "execute",
      },
      state: "pending",
      created_at: "2026-05-17T00:00:00Z",
      options: [{ id: "allow_once", label: "Allow Once", kind: "allow" }],
    },
  };
}

function thoughtMessage(id: string, text: string): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "thought",
    message_id: id,
    message: {
      kind: "thought",
      id,
      text,
      created_at: "2026-05-17T00:00:00Z",
    },
  };
}

function activityMessage(id: string, title: string, status: "running" | "completed" | "error", steps: ActivityStep[]): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "activity",
    message_id: id,
    message: {
      kind: "activity",
      id,
      title,
      status,
      created_at: "2026-05-17T00:00:00Z",
      collapsed: true,
      steps,
    },
  };
}

function settingsAgents(agentIds: string[] = ["codex"]): AgentSettingsRecord[] {
  return agentIds.map((id) => ({
    id,
    label: id === "codex" ? "Codex" : "Custom",
    enabled: true,
    scope: "global",
    source_kind: id === "codex" ? "built_in" : "custom",
    icon: id === "codex" ? "openai" : "bot",
    transport: "stdio",
    status: "unprobed",
    launch_label: id === "codex" ? "Built-in stdio launch policy" : "custom-agent",
    description: id === "codex" ? "Built-in Agent." : "Custom Agent.",
    capabilities: ["ACP stdio"],
    auth_methods: [],
  }));
}
