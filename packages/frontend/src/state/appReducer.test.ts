import { describe, expect, it } from "vitest";
import type { ActivityStep, AgentSettingsRecord, ChatMessage, MessagePage, TaskSnapshot, TaskSummary } from "@openaide/app-shell-contracts";
import { appReducer } from "./appReducer";
import { renderedChat } from "./chatPaging";
import { createInitialState } from "./store";

type PermissionChatMessage = ChatMessage & {
  message: Extract<ChatMessage["message"], { kind: "permission" }>;
};

describe("app reducer composer state", () => {
  it("keeps a config error until its timer or a changed Agent catalog clears it", () => {
    const initial = snapshot("task_1");
    initial.agent_config = configCatalog("off");
    let state = appReducer(createInitialState(), { type: "snapshot", intent: "open", snapshot: initial });
    state = appReducer(state, {
      type: "taskInput:configError",
      taskId: "task_1",
      mutationId: "mutation-1",
      message: "Agent option update timed out.",
      catalog: initial.agent_config,
    });

    const unchanged = snapshot("task_1", [], 2);
    unchanged.agent_config = configCatalog("off");
    state = appReducer(state, { type: "snapshot", intent: "refresh", snapshot: unchanged });
    expect(state.taskInputs.task_1.configError?.message).toBe("Agent option update timed out.");

    const loading = snapshot("task_1", [], 3);
    loading.agent_config = { ...configCatalog("off"), status: "loading", options: [] };
    state = appReducer(state, { type: "snapshot", intent: "refresh", snapshot: loading });
    expect(state.taskInputs.task_1.configError?.message).toBe("Agent option update timed out.");

    state = appReducer(state, {
      type: "taskInput:configError:clear",
      taskId: "task_1",
      mutationId: "another-mutation",
    });
    expect(state.taskInputs.task_1.configError?.message).toBe("Agent option update timed out.");

    const changed = snapshot("task_1", [], 4);
    changed.agent_config = configCatalog("on");
    state = appReducer(state, { type: "snapshot", intent: "refresh", snapshot: changed });
    expect(state.taskInputs.task_1.configError).toBeUndefined();
  });

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
      initialProjectId: "project-2",
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
      initialProjectId: "project-1",
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
      initialProjectId: "project-1",
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
      initialProjectId: "project-1",
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

  it("keeps expanded session history visible while refreshing it", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "newTask:nativeSessions:result",
      append: false,
      result: {
        agent_id: "codex",
        sessions: [
          { session_id: "session_1", cwd: "/workspace/app", title: "Recent" },
          { session_id: "session_2", cwd: "/workspace/app", title: "Older" },
        ],
      },
    });

    state = appReducer(state, { type: "newTask:nativeSessions:start", append: false });

    expect(state.newTask.nativeSessions.loading).toBe(true);
    expect(state.newTask.nativeSessions.items.map((session) => session.session_id)).toEqual([
      "session_1",
      "session_2",
    ]);
  });

  it("keeps a newer first-send submission locked when session-list loading fails", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "newTask:nativeSessions:start", append: false });
    state = appReducer(state, {
      type: "submit:start",
      prompt: "Start this Task once",
      context: [],
    });

    state = appReducer(state, {
      type: "newTask:nativeSessions:listError",
      message: "Unable to load Agent session history.",
    } as never);

    expect(state.newTask.submitting).toBe(true);
    expect(state.newTask.nativeSessions).toMatchObject({
      error: "Unable to load Agent session history.",
      loaded: true,
      loading: false,
    });
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
    state = appReducer(state, {
      type: "newTask:nativeSessions:error",
      sessionId: "session_2",
      message: "Unable to open task.",
    });
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

  it("clears an earlier adoption error when retrying that native session", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "newTask:nativeSessions:adopt", sessionId: "session_1" });
    state = appReducer(state, {
      type: "newTask:nativeSessions:error",
      sessionId: "session_1",
      message: "Unable to open task.",
    });

    state = appReducer(state, { type: "newTask:nativeSessions:adopt", sessionId: "session_1" });

    expect(state.newTask.nativeSessions.error).toBeUndefined();
    expect(state.newTask.nativeSessions.adoptingSessionId).toBe("session_1");
  });

  it("does not let stale native-session adoption completion unlock a newer first send", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "newTask:nativeSessions:result",
      append: false,
      result: {
        agent_id: "codex",
        sessions: [{ session_id: "session_1", cwd: "/workspace/app", title: "Existing" }],
      },
    });
    state = appReducer(state, { type: "newTask:nativeSessions:adopt", sessionId: "session_1" });
    state = appReducer(state, { type: "submit:cancel" });
    state = appReducer(state, {
      type: "submit:start",
      prompt: "Start newer Task",
      context: [],
    });

    state = appReducer(state, { type: "newTask:nativeSessions:remove", sessionId: "session_1" });

    expect(state.newTask.submitting).toBe(true);
    expect(state.newTask.nativeSessions.adoptingSessionId).toBeUndefined();
    expect(state.newTask.nativeSessions.items).toEqual([]);
  });

  it("ignores a stale native-session adoption error after a newer first send starts", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "newTask:nativeSessions:adopt", sessionId: "session_1" });
    state = appReducer(state, { type: "submit:cancel" });
    state = appReducer(state, {
      type: "submit:start",
      prompt: "Start newer Task",
      context: [],
    });

    state = appReducer(state, {
      type: "newTask:nativeSessions:error",
      sessionId: "session_1",
      message: "Old adoption failed.",
    } as never);

    expect(state.newTask.submitting).toBe(true);
    expect(state.newTask.nativeSessions.error).toBeUndefined();
  });

  it("keeps native-session adoption ownership through unrelated Task snapshots", () => {
    let state = createInitialState();
    state.activeTaskId = "task_background";
    state = appReducer(state, { type: "newTask:nativeSessions:adopt", sessionId: "session_1" });

    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: snapshot("task_background"),
    });

    expect(state.newTask.submitting).toBe(true);
    expect(state.newTask.nativeSessions.adoptingSessionId).toBe("session_1");
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

    state = appReducer(state, {
      type: "taskInput:submit",
      taskId: "task_1",
    });
    expect(state.taskInputs.task_1.prompt).toBe("Use context");
    expect(state.taskInputs.task_1.context).toHaveLength(1);
    expect(state.taskInputs.task_1.pending?.prompt).toBe("Use context");

    state = appReducer(state, {
      type: "taskInput:sendError",
      taskId: "task_1",
      message: "Send failed",
    } as never);
    expect(state.taskInputs.task_1.prompt).toBe("Use context");
    expect(state.taskInputs.task_1.context[0]).toMatchObject({ label: "src/main.rs" });
    expect(state.taskInputs.task_1.error).toBe("Send failed");
    expect(state.taskInputs.task_1.pending).toBeUndefined();
  });

  it("keeps the exact pending send when stopping the task fails", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "taskInput:submit",
      taskId: "task_1",
      input: { prompt: "Keep this pending", context: [] },
    });

    state = appReducer(state, {
      type: "taskInput:cancelError",
      taskId: "task_1",
      message: "Unable to stop task.",
    } as never);

    expect(state.taskInputs.task_1).toEqual({
      prompt: "Keep this pending",
      context: [],
      error: "Unable to stop task.",
      pending: {
        prompt: "Keep this pending",
        context: [],
        state: "sending",
      },
    });
  });

  it("keeps the exact pending send when an unrelated Task operation fails", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "taskInput:submit",
      taskId: "task_1",
      input: { prompt: "Keep this exact send", context: [] },
    });

    state = appReducer(state, {
      type: "taskInput:error",
      taskId: "task_1",
      message: "History refresh failed.",
    });

    expect(state.taskInputs.task_1).toEqual({
      prompt: "Keep this exact send",
      context: [],
      error: "History refresh failed.",
      pending: {
        prompt: "Keep this exact send",
        context: [],
        state: "sending",
      },
    });
  });

  it("clears local follow-up drafts for a task", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Do not send" });

    state = appReducer(state, { type: "taskInput:clear", taskId: "task_1" });

    expect(state.taskInputs.task_1).toBeUndefined();
  });

  it("settles only the send attempt explicitly accepted by its user message result", () => {
    let state = createInitialState();
    state = { ...state, activeTaskId: "task_1" };
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Repeat this" });
    state = appReducer(state, {
      type: "taskInput:submit",
      taskId: "task_1",
    });

    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: snapshot("task_1", [userMessage("older-message", "Repeat this")], 2),
    });

    expect(state.taskInputs.task_1.pending?.prompt).toBe("Repeat this");

    state = appReducer(state, {
      type: "taskSend:accepted",
      taskId: "task_1",
      userMessageId: "accepted-message" as never,
    });

    expect(state.taskInputs.task_1).toEqual({
      prompt: "",
      context: [],
      acceptedUserMessageId: "accepted-message",
    });
  });

  it("does not settle a pending follow-up from matching snapshot content", () => {
    let state = createInitialState();
    state = { ...state, activeTaskId: "task_1" };
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Accepted" });
    state = appReducer(state, {
      type: "taskInput:submit",
      taskId: "task_1",
    });

    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: snapshot("task_1", [userMessage("user_1", "Accepted")]),
    });

    expect(state.taskInputs.task_1.prompt).toBe("Accepted");
    expect(state.taskInputs.task_1.context).toHaveLength(0);
  });

  it("preserves an opened-task draft that matches historical chat", () => {
    let state = createInitialState();
    state = { ...state, activeTaskId: "task_1" };
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Explain this screenshot" });
    state = appReducer(state, {
      type: "taskInput:attachment:add",
      taskId: "task_1",
      attachment: { kind: "file", label: "screenshot.png", path: "/workspace/screenshot.png" },
    });

    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: snapshot("task_1", [userMessage("user_1", "Explain this screenshot", 1)], 2),
    });

    expect(state.taskInputs.task_1.prompt).toBe("Explain this screenshot");
    expect(state.taskInputs.task_1.context).toHaveLength(1);
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
    state = appReducer(state, {
      type: "taskInput:submit",
      taskId: "task_1",
    });

    const accepted = snapshot("task_1", [userMessage("user_1", "Accepted")], 2);
    state = appReducer(state, { type: "snapshot", intent: "refresh", snapshot: accepted });
    state = appReducer(state, {
      type: "taskSend:accepted",
      taskId: "task_1",
      userMessageId: "user_1" as never,
    });
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

  it("does not settle pending input from an incomplete matching refresh", () => {
    let state = createInitialState();
    state = { ...state, activeTaskId: "task_1" };
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Committed" });
    state = appReducer(state, {
      type: "taskInput:submit",
      taskId: "task_1",
    });
    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: snapshot("task_1", [
        userMessage("user_1", "Committed"),
        chatMessage("agent_1", "working"),
      ], 2),
    });
    state = appReducer(state, {
      type: "taskSend:accepted",
      taskId: "task_1",
      userMessageId: "user_1" as never,
    });
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Committed again" });
    state = appReducer(state, { type: "taskInput:submit", taskId: "task_1" });

    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: snapshot("task_1", [userMessage("user_2", "Committed again")], 2),
    });

    expect(state.snapshot?.chat.items.map((item) => item.message_id)).toEqual(["user_1", "agent_1"]);
    expect(state.taskInputs.task_1.pending?.prompt).toBe("Committed again");
  });

  it("keeps pending follow-up input through no-message preparation snapshots", () => {
    let state = createInitialState();
    state = { ...state, activeTaskId: "task_1" };
    state = appReducer(state, { type: "taskInput:prompt", taskId: "task_1", prompt: "Pending send" });
    state = appReducer(state, { type: "taskInput:submit", taskId: "task_1" });

    state = appReducer(state, { type: "snapshot", intent: "refresh", snapshot: noMessageSnapshot("task_1") });

    expect(state.taskInputs.task_1.prompt).toBe("Pending send");
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

    expect(state.newTask.prompt).toBe("Build the thing");
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

  it("invalidates a submitted New Task attachment through the root reducer", () => {
    let state = createInitialState();
    state.newTask.prompt = "Inspect this";
    state.newTask.context = [{
      app_server_handle_id: "attachment-handle-1" as never,
      kind: "file",
      label: "trace.png",
      local_id: "attachment-1",
    }];
    state = appReducer(state, { type: "submit:start" });

    state = appReducer(state, {
      type: "submit:attachments:invalidate",
      taskId: "task-new",
      message: "Attachment handle expired.",
    });

    expect(state.newTask.submitting).toBe(false);
    expect(state.newTask.error).toBe("Attachment handle expired.");
    expect(state.newTask.context[0]).toMatchObject({ validation_error: "Attachment handle expired." });
    expect(state.newTask.context[0]).not.toHaveProperty("app_server_handle_id");
    expect(state.taskInputs["task-new"]?.context[0]).toMatchObject({ validation_error: "Attachment handle expired." });
    expect(state.taskInputs["task-new"]?.context[0]).not.toHaveProperty("app_server_handle_id");
  });

  it("clears accepted new-task text and attachments only after the send result", () => {
    let state = createInitialState();
    state.newTask.prompt = "Explain this";
    state.newTask.context = [{
      kind: "file",
      label: "screenshot.png",
      local_id: "attachment_1",
    }];
    state = appReducer(state, { type: "submit:start" });

    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: snapshot("task_new", [userMessage("user_1", "Explain this", 1)]),
    });

    expect(state.newTask.prompt).toBe("Explain this");
    expect(state.newTask.context).toHaveLength(1);

    state = appReducer(state, {
      type: "taskSend:accepted",
      taskId: "task_new",
      userMessageId: "user_1" as never,
    });

    expect(state.newTask.prompt).toBe("");
    expect(state.newTask.context).toEqual([]);
    expect(state.newTask.pending).toBeUndefined();
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

  it("keeps a client-private New Task out of visible Task state", () => {
    let state = createInitialState();
    const newTask = { ...noMessageSnapshot("task_new"), lifecycle: "new" as const };

    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: newTask });

    expect(state.tasks).toEqual([]);
    expect(state.taskListCache).toEqual({});
    expect(state.activeTaskId).toBeUndefined();
    expect(state.snapshot).toBeUndefined();
    expect(state.taskSnapshots.task_new).toBeUndefined();
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

  it("does not rebuild task navigation for a chat-only snapshot revision", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_1") });
    const tasks = state.tasks;

    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: snapshot("task_1", [], 2),
    });

    expect(state.tasks).toBe(tasks);
    expect(state.snapshot?.revision).toBe(2);
  });

  it("does not resurrect a task omitted by the authoritative navigation snapshot", () => {
    let state = createInitialState();
    state.tasks = [
      taskSummary("task_1"),
      { ...taskSummary("task_new"), has_messages: false, title: "New task" },
    ];
    state.taskInputs.task_new = {
      prompt: "",
      context: [],
      pending: { prompt: "Ship it", context: [], state: "sending" },
    };

    state = appReducer(state, { type: "tasks", archived: false, tasks: [taskSummary("task_1")] });

    expect(state.tasks.map((task) => task.task_id)).toEqual(["task_1"]);
    expect(state.taskListCache.active?.map((task) => task.task_id)).toEqual(["task_1"]);
  });

  it("keeps a late background snapshot out of an authoritative list that omitted its Task", () => {
    let state = createInitialState();
    state = {
      ...state,
      activeTaskId: "task_1",
      tasks: [taskSummary("task_1"), taskSummary("task_removed")],
      taskListCache: {
        active: [taskSummary("task_1"), taskSummary("task_removed")],
      },
    };
    state = appReducer(state, {
      type: "tasks",
      archived: false,
      tasks: [taskSummary("task_1")],
    });

    const lateSnapshot = {
      ...snapshot("task_removed"),
      task: { ...taskSummary("task_removed"), title: "Late refresh" },
    };
    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: lateSnapshot,
    });

    expect(state.tasks.map((task) => task.task_id)).toEqual(["task_1"]);
    expect(state.taskListCache.active?.map((task) => task.task_id)).toEqual(["task_1"]);
    expect(state.taskSnapshots.task_removed?.task.title).toBe("Late refresh");
  });

  it("updates an active cache row without inserting it into the visible Archived slice", () => {
    let state = createInitialState();
    const archivedTask = taskSummary("task_archived");
    state = {
      ...state,
      activeTaskId: "task_archived",
      showArchived: true,
      tasks: [archivedTask],
      taskListCache: {
        active: [taskSummary("task_active")],
        archived: [archivedTask],
      },
    };

    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: {
        ...snapshot("task_active"),
        task: { ...taskSummary("task_active"), title: "Updated active" },
      },
    });

    expect(state.tasks).toEqual([archivedTask]);
    expect(state.taskListCache.archived).toEqual([archivedTask]);
    expect(state.taskListCache.active?.[0].title).toBe("Updated active");
  });

  it("keeps active subscription updates out of the visible Archived slice", () => {
    let state = createInitialState();
    const archivedTask = { ...taskSummary("task_archived"), title: "Archived Task" };
    state = {
      ...state,
      showArchived: true,
      tasks: [archivedTask],
      taskListCache: { archived: [archivedTask] },
    };
    const activeTask = { ...taskSummary("task_active"), title: "Active Task" };

    state = appReducer(state, {
      type: "tasks",
      archived: false,
      tasks: [activeTask],
    });

    expect(state.tasks).toEqual([archivedTask]);
    expect(state.taskListCache.archived).toEqual([archivedTask]);
    expect(state.taskListCache.active).toEqual([activeTask]);
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

    state = appReducer(state, {
      type: "taskScroll:record",
      taskId: "task_1",
      scrollState: { ownership: "reading", scrollTop: 320 },
    });
    state = appReducer(state, {
      type: "taskScroll:record",
      taskId: "task_2",
      scrollState: { ownership: "following", scrollTop: 80 },
    });

    expect(state.taskChatScrollStates).toEqual({
      task_1: { ownership: "reading", scrollTop: 320 },
      task_2: { ownership: "following", scrollTop: 80 },
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
    state = appReducer(state, {
      type: "tasks",
      archived: false,
      tasks: [taskSummary("task_active")],
    });
    state = appReducer(state, { type: "archive:set", showArchived: true });
    state = appReducer(state, {
      type: "tasks",
      archived: true,
      tasks: [taskSummary("task_archived")],
    });

    state = appReducer(state, { type: "archive:set", showArchived: false });
    expect(state.tasks.map((task) => task.task_id)).toEqual(["task_active"]);

    state = appReducer(state, { type: "archive:set", showArchived: true });
    expect(state.tasks.map((task) => task.task_id)).toEqual(["task_archived"]);
  });

  it("stores task-list errors until a list or archive-mode change is accepted", () => {
    let state = createInitialState();

    state = appReducer(state, { type: "tasks:error", message: "Unable to load tasks" });
    expect(state.taskListError).toBe("Unable to load tasks");

    state = appReducer(state, { type: "tasks", archived: false, tasks: [taskSummary("task_1")] });
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
    state = appReducer(state, { type: "chatPage:start", taskId: "task_1", requestGeneration: 1 });
    state = appReducer(state, {
      type: "chatPage:result",
      taskId: "task_1",
      requestGeneration: 1,
      page: page("task_1", [chatMessage("m1", "older 1"), chatMessage("m3", "tail 3")], false),
    });

    const chat = renderedChat(taskSnapshot, state.chatPages.task_1);

    expect(chat.items.map((message) => message.message_id)).toEqual(["m1", "m3", "m4"]);
    expect(chat.hasBefore).toBe(false);
    expect(state.chatPages.task_1.pending).toBe(false);
    expect(state.chatPages.task_1.requestGeneration).toBe(1);
  });

  it("retains the open Task chat window when a live snapshot contains only a newer tail", () => {
    let state = createInitialState();
    const opened = snapshot("task_1", [
      chatMessage("m1", "user context"),
      chatMessage("m2", "earlier response"),
    ]);
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: opened });

    const refreshed = snapshot("task_1", [
      chatMessage("m2", "updated response"),
      chatMessage("m3", "live update"),
    ]);
    state = appReducer(state, { type: "snapshot", intent: "refresh", snapshot: refreshed });

    const chat = renderedChat(state.snapshot!, state.chatPages.task_1);
    expect(chat.items.map((message) => message.message_id)).toEqual(["m1", "m2", "m3"]);
    expect(chat.items.map((message) => message.message.kind === "agent_message"
      && message.message.parts[0]?.kind === "text" ? message.message.parts[0].text : "")).toEqual([
      "user context",
      "updated response",
      "live update",
    ]);
  });

  it("drops retained rows when synchronized history is authoritatively replaced", () => {
    let state = createInitialState();
    const checking = snapshot("task_1", [chatMessage("old-tail", "stale tail")]);
    checking.history_sync = { state: "syncing", generation: 2 };
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: checking });
    state = appReducer(state, { type: "chatPage:start", taskId: "task_1", requestGeneration: 1 });
    state = appReducer(state, {
      type: "chatPage:result",
      taskId: "task_1",
      requestGeneration: 1,
      page: page("task_1", [chatMessage("old-page", "stale page")], false),
    });

    const replacement = snapshot("task_1", [chatMessage("native-row", "native history")], 2);
    replacement.history_sync = { state: "syncing", generation: 2 };
    state = appReducer(state, { type: "snapshot", intent: "refresh", snapshot: replacement });
    expect(state.chatPages.task_1).toBeDefined();

    const completed = { ...replacement, history_sync: { state: "updated" as const, generation: 2 } };
    state = appReducer(state, { type: "snapshot", intent: "refresh", snapshot: completed });

    expect(state.chatPages.task_1).toBeUndefined();
    expect(renderedChat(state.snapshot!, state.chatPages.task_1).items.map((item) => item.message_id)).toEqual([
      "native-row",
    ]);
  });

  it("retains loaded history when ordinary live Chat grows during send synchronization", () => {
    let state = createInitialState();
    const syncing = snapshot("task_1", [chatMessage("old-tail", "existing tail")]);
    syncing.history_sync = { state: "syncing", generation: 2 };
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: syncing });
    state = appReducer(state, { type: "chatPage:start", taskId: "task_1", requestGeneration: 1 });
    state = appReducer(state, {
      type: "chatPage:result",
      taskId: "task_1",
      requestGeneration: 1,
      page: page("task_1", [chatMessage("old-page", "loaded history")], false),
    });

    const liveGrowth = snapshot("task_1", [
      chatMessage("old-tail", "existing tail"),
      chatMessage("live-row", "live response"),
    ], 2);
    liveGrowth.history_sync = { state: "syncing", generation: 2 };
    state = appReducer(state, { type: "snapshot", intent: "refresh", snapshot: liveGrowth });

    expect(state.chatPages.task_1?.olderItems.map((item) => item.message_id)).toEqual([
      "old-page",
    ]);
  });

  it("drops retained rows when resubscribe first observes a newer completed history generation", () => {
    let state = createInitialState();
    const opened = snapshot("task_1", [chatMessage("old-tail", "stale tail")]);
    opened.history_sync = { state: "idle", generation: 4 };
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: opened });
    state = appReducer(state, { type: "chatPage:start", taskId: "task_1", requestGeneration: 1 });
    state = appReducer(state, {
      type: "chatPage:result",
      taskId: "task_1",
      requestGeneration: 1,
      page: page("task_1", [chatMessage("old-page", "stale page")], false),
    });

    const replacement = snapshot("task_1", [chatMessage("native-row", "native history")], 5);
    replacement.history_sync = { state: "updated", generation: 5 };
    state = appReducer(state, { type: "snapshot", intent: "refresh", snapshot: replacement });

    expect(state.chatPages.task_1).toBeUndefined();
    expect(renderedChat(state.snapshot!, state.chatPages.task_1).items.map((item) => item.message_id)).toEqual([
      "native-row",
    ]);
  });

  it("does not let a late task-open response regress a completed history sync generation", () => {
    let state = createInitialState();
    const baseline = snapshot("task_1", [chatMessage("m1", "Current history")], 2);
    baseline.history_sync = { state: "idle", generation: 6 };
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: baseline });

    const completed = snapshot("task_1", [chatMessage("m1", "Current history")], 2);
    completed.history_sync = { state: "idle", generation: 7 };
    state = appReducer(state, { type: "snapshot", intent: "refresh", snapshot: completed });

    const staleOpen = snapshot("task_1", [chatMessage("m1", "Current history")], 2);
    staleOpen.history_sync = { state: "syncing", generation: 7 };
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: staleOpen });

    expect(state.snapshot?.history_sync).toEqual({ state: "idle", generation: 7 });
  });

  it("merges durable snapshot growth without regressing its independent history sync clock", () => {
    let state = createInitialState();
    const completed = snapshot("task_1", [chatMessage("m1", "Current history")], 2);
    completed.history_sync = { state: "updated", generation: 8 };
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: completed });

    const durableGrowth = snapshot("task_1", [
      chatMessage("m1", "Current history"),
      chatMessage("m2", "New durable row"),
    ], 3);
    durableGrowth.history_sync = { state: "syncing", generation: 7 };
    state = appReducer(state, { type: "snapshot", intent: "refresh", snapshot: durableGrowth });

    expect(state.snapshot?.revision).toBe(3);
    expect(state.snapshot?.chat.items.map((item) => item.message_id)).toEqual(["m1", "m2"]);
    expect(state.snapshot?.history_sync).toEqual({ state: "updated", generation: 8 });
  });

  it("settles history sync from a subscription replica behind the durable snapshot", () => {
    let state = createInitialState();
    const durable = snapshot("task_1", [
      chatMessage("m1", "Existing history"),
      chatMessage("m2", "New durable row"),
    ], 3);
    durable.history_sync = { state: "syncing", generation: 7 };
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: durable });

    const subscriptionCompletion = snapshot("task_1", [chatMessage("m1", "Existing history")], 2);
    subscriptionCompletion.history_sync = { state: "idle", generation: 7 };
    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: subscriptionCompletion,
    });

    expect(state.snapshot?.revision).toBe(3);
    expect(state.snapshot?.chat.items.map((item) => item.message_id)).toEqual(["m1", "m2"]);
    expect(state.snapshot?.history_sync).toEqual({ state: "idle", generation: 7 });
  });

  it("accepts a restarted App Server's new history clock and rejects its predecessor's late results", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "appServer:replica",
      epoch: 1,
      stateRootId: "state_root_1",
    });
    const previousProcess = snapshot("task_1", [chatMessage("m1", "Durable history")], 3);
    previousProcess.history_sync = { state: "updated", generation: 8 };
    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: previousProcess,
      replicaEpoch: 1,
    });
    state = appReducer(state, {
      type: "appServer:replica",
      epoch: 2,
      stateRootId: "state_root_1",
    });

    const restartedProcess = snapshot("task_1", [chatMessage("m1", "Durable history")], 3);
    restartedProcess.history_sync = { state: "syncing", generation: 1 };
    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: restartedProcess,
      replicaEpoch: 2,
    });

    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: previousProcess,
      replicaEpoch: 1,
    });

    expect(state.snapshot?.history_sync).toEqual({ state: "syncing", generation: 1 });

    const settledRestart = { ...restartedProcess, history_sync: { state: "idle" as const, generation: 1 } };
    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: settledRestart,
      replicaEpoch: 2,
    });
    expect(state.snapshot?.history_sync).toEqual({ state: "idle", generation: 1 });
  });

  it("preserves loaded chat pages across a same-process stream resubscribe", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "appServer:replica",
      epoch: 1,
      stateRootId: "state_root_1",
    });
    const baseline = snapshot("task_1", [chatMessage("tail", "Current tail")], 3);
    baseline.history_sync = { state: "updated", generation: 8 };
    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: baseline,
      replicaEpoch: 1,
    });
    state = appReducer(state, { type: "chatPage:start", taskId: "task_1", requestGeneration: 1, replicaEpoch: 1 });
    state = appReducer(state, {
      type: "chatPage:result",
      taskId: "task_1",
      requestGeneration: 1,
      page: page("task_1", [chatMessage("older", "Loaded history")], false),
      replicaEpoch: 1,
    });

    const resubscribed = snapshot("task_1", [chatMessage("tail", "Fresh durable tail")], 4);
    resubscribed.history_sync = { state: "syncing", generation: 7 };
    state = appReducer(state, {
      type: "appServer:replica",
      epoch: 1,
      stateRootId: "state_root_1",
    });
    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: resubscribed,
      replicaEpoch: 1,
    });

    expect(state.chatPages.task_1?.olderItems.map((item) => item.message_id)).toEqual(["older"]);
    expect(state.snapshot).toMatchObject({
      revision: 4,
      history_sync: { state: "updated", generation: 8 },
    });
  });

  it("invalidates process-owned requests, options, sessions, and attachment handles after restart", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "appServer:replica",
      epoch: 1,
      stateRootId: "state_root_1",
    });
    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: snapshot("task_1", [chatMessage("tail", "Keep durable chat")], 4),
      replicaEpoch: 1,
    });
    const attachment = {
      kind: "file" as const,
      label: "trace.png",
      local_id: "attachment_1",
      app_server_handle_id: "handle_1" as never,
    };
    state = appReducer(state, {
      type: "taskInput:submit",
      taskId: "task_1",
      input: { prompt: "Keep exact send", context: [attachment] },
      replicaEpoch: 1,
    });
    state = appReducer(state, {
      type: "taskInput:attachment:addAppServer",
      taskId: "task_2",
      attachment,
      replicaEpoch: 1,
    });
    state.newTask = {
      ...state.newTask,
      prompt: "Keep new draft",
      context: [attachment],
      configOptions: {
        agent_id: "codex",
        status: "ready",
        options: [],
      },
      selection: {
        ...state.newTask.selection,
        configOptions: { model: "old-model" },
      },
      nativeSessions: {
        items: [{ session_id: "old_session", cwd: "/workspace" }],
        loading: false,
        loaded: true,
      },
    };
    state.permissionResponses.permission_1 = { responding: true };
    state.questionResponses.question_1 = { responding: true };
    state.toolDetails["task_1\u0000artifact_1"] = { loading: false };

    state = appReducer(state, {
      type: "appServer:replica",
      epoch: 2,
      stateRootId: "state_root_1",
    });

    expect(state.snapshot?.chat.items.map((item) => item.message_id)).toEqual(["tail"]);
    expect(state.permissionResponses).toEqual({});
    expect(state.questionResponses).toEqual({});
    expect(state.toolDetails).toEqual({});
    expect(state.newTask).toMatchObject({
      prompt: "Keep new draft",
      configOptions: undefined,
      configOptionsLoading: false,
      nativeSessions: { items: [], loading: false, loaded: false },
      selection: { configOptions: {} },
    });
    expect(state.newTask.context[0]).toMatchObject({
      validation_error: "Attachment must be reselected after App Server restart.",
    });
    expect(state.newTask.context[0]).not.toHaveProperty("app_server_handle_id");
    expect(state.taskInputs.task_1).toMatchObject({
      prompt: "Keep exact send",
      error: "App Server restarted. Review the draft before sending again.",
    });
    expect(state.taskInputs.task_1.context[0]).not.toHaveProperty("app_server_handle_id");
    expect(state.taskInputs.task_1.pending).toBeUndefined();
    expect(state.taskInputs.task_2.context[0]).toMatchObject({
      validation_error: "Attachment must be reselected after App Server restart.",
    });
    expect(state.taskInputs.task_2.context[0]).not.toHaveProperty("app_server_handle_id");
  });

  it("drops old paging rows when a replacement process reports updated history", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "appServer:replica",
      epoch: 1,
      stateRootId: "state_root_1",
    });
    const oldProcess = snapshot("task_1", [chatMessage("old-tail", "Old tail")], 3);
    oldProcess.history_sync = { state: "idle", generation: 8 };
    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: oldProcess,
      replicaEpoch: 1,
    });
    state = appReducer(state, { type: "chatPage:start", taskId: "task_1", requestGeneration: 1, replicaEpoch: 1 });
    state = appReducer(state, {
      type: "chatPage:result",
      taskId: "task_1",
      requestGeneration: 1,
      page: page("task_1", [chatMessage("old-page", "Old page")], false),
      replicaEpoch: 1,
    });
    state = appReducer(state, {
      type: "appServer:replica",
      epoch: 2,
      stateRootId: "state_root_1",
    });

    const replacement = snapshot("task_1", [chatMessage("native-row", "Native history")], 4);
    replacement.history_sync = { state: "updated", generation: 1 };
    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: replacement,
      replicaEpoch: 2,
    });

    expect(state.chatPages.task_1).toBeUndefined();
    expect(state.snapshot?.chat.items.map((item) => item.message_id)).toEqual(["native-row"]);
  });

  it("clears state-root-owned caches before accepting colliding task identities", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "appServer:replica",
      epoch: 1,
      stateRootId: "state_root_1",
    });
    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: snapshot("task_1", [chatMessage("tail", "Old root")], 9),
      replicaEpoch: 1,
    });
    state = appReducer(state, {
      type: "taskInput:prompt",
      taskId: "task_1",
      prompt: "Old root draft",
      replicaEpoch: 1,
    });
    state = appReducer(state, { type: "chatPage:start", taskId: "task_1", requestGeneration: 1, replicaEpoch: 1 });
    state = appReducer(state, {
      type: "chatPage:result",
      taskId: "task_1",
      requestGeneration: 1,
      page: page("task_1", [chatMessage("older", "Old root page")], false),
      replicaEpoch: 1,
    });
    state = appReducer(state, {
      type: "projects",
      projects: [{ projectId: "project_old", label: "Old root" }],
      replicaEpoch: 1,
    });
    state = appReducer(state, { type: "prompt", prompt: "Old root prompt", replicaEpoch: 1 });
    state = appReducer(state, { type: "settings:error", message: "Old root error", replicaEpoch: 1 });
    state = appReducer(state, { type: "search:set", query: "keep search" });
    state = appReducer(state, { type: "archive:set", showArchived: true });
    state = appReducer(state, {
      type: "workspace:roots",
      roots: [{ path: "/workspace/current", label: "Current workspace" }],
    });

    state = appReducer(state, {
      type: "appServer:replica",
      epoch: 2,
      stateRootId: "state_root_2",
    });

    expect(state).toMatchObject({
      activeTaskId: undefined,
      projects: [],
      projectsLoaded: false,
      tasks: [],
      taskListCache: {},
      taskInputs: {},
      taskSnapshots: {},
      taskSnapshotReplicaEpochs: {},
      chatPages: {},
      toolDetails: {},
      settings: { loading: false },
      newTask: { prompt: "", submitting: false },
      searchQuery: "keep search",
      showArchived: true,
      workspaceRoots: [{ path: "/workspace/current", label: "Current workspace" }],
    });
    expect(state.settings.error).toBeUndefined();

    state = appReducer(state, {
      type: "taskInput:prompt",
      taskId: "task_1",
      prompt: "Late old-root draft",
      replicaEpoch: 1,
    });
    expect(state.taskInputs.task_1).toBeUndefined();

    const collidingTask = snapshot("task_1", [chatMessage("new", "New root")], 1);
    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: collidingTask,
      replicaEpoch: 2,
    });

    expect(state.snapshot?.revision).toBe(1);
    expect(state.snapshot?.chat.items.map((item) => item.message_id)).toEqual(["new"]);
  });

  it("restores the retained Chat window and scroll position after switching Tasks", () => {
    let state = createInitialState();
    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: snapshot("task_1", [chatMessage("m1", "Earlier")]),
    });
    state = appReducer(state, {
      type: "snapshot",
      intent: "refresh",
      snapshot: snapshot("task_1", [chatMessage("m2", "Latest")]),
    });
    state = appReducer(state, {
      type: "taskScroll:record",
      taskId: "task_1",
      scrollState: { ownership: "reading", scrollTop: 320 },
    });
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_2") });

    state = appReducer(state, { type: "selection:set", taskId: "task_1" });

    const chat = renderedChat(state.snapshot!, state.chatPages.task_1);
    expect(chat.items.map((message) => message.message_id)).toEqual(["m1", "m2"]);
    expect(state.taskChatScrollStates.task_1).toEqual({ ownership: "reading", scrollTop: 320 });
  });

  it("preserves adjacent persisted Agent rows as distinct protocol identities", () => {
    const taskSnapshot = snapshot("task_1", [
      chatMessage("m1", "Called"),
      chatMessage("m2", " `"),
      chatMessage("m3", "pwd"),
      chatMessage("m4", "`:"),
      chatMessage("m5", " `/work"),
      chatMessage("m6", "space"),
      chatMessage("m7", "/pro"),
      chatMessage("m8", "ject"),
      chatMessage("m9", "`"),
    ]);

    const chat = renderedChat(taskSnapshot, undefined);

    expect(chat.items.map((item) => item.message_id)).toEqual([
      "m1", "m2", "m3", "m4", "m5", "m6", "m7", "m8", "m9",
    ]);
    expect(chat.items.map((item) => item.message)).toMatchObject([
      { kind: "agent_message", parts: [{ kind: "text", text: "Called" }] },
      { kind: "agent_message", parts: [{ kind: "text", text: " `" }] },
      { kind: "agent_message", parts: [{ kind: "text", text: "pwd" }] },
      { kind: "agent_message", parts: [{ kind: "text", text: "`:" }] },
      { kind: "agent_message", parts: [{ kind: "text", text: " `/work" }] },
      { kind: "agent_message", parts: [{ kind: "text", text: "space" }] },
      { kind: "agent_message", parts: [{ kind: "text", text: "/pro" }] },
      { kind: "agent_message", parts: [{ kind: "text", text: "ject" }] },
      { kind: "agent_message", parts: [{ kind: "text", text: "`" }] },
    ]);
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

  it("ignores stale earlier pages for a task that is no longer selected", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_current") });

    state = appReducer(state, {
      type: "chatPage:result",
      taskId: "task_old",
      requestGeneration: 1,
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
      requestGeneration: 1,
      message: "Unable to load earlier messages",
    });

    expect(state.chatPages.task_old).toBeUndefined();
  });

  it("settles an in-flight earlier page after the user switches Tasks", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_old") });
    state = appReducer(state, { type: "chatPage:start", taskId: "task_old", requestGeneration: 1 });
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_current") });

    state = appReducer(state, {
      type: "chatPage:result",
      taskId: "task_old",
      requestGeneration: 1,
      page: page("task_old", [chatMessage("old", "Earlier")], false),
    });

    expect(state.chatPages.task_old).toMatchObject({
      pending: false,
      hasBefore: false,
    });
    expect(state.chatPages.task_old?.olderItems.map((item) => item.message_id)).toEqual(["old"]);
  });

  it("settles an in-flight earlier-page failure after the user switches Tasks", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_old") });
    state = appReducer(state, { type: "chatPage:start", taskId: "task_old", requestGeneration: 1 });
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_current") });

    state = appReducer(state, {
      type: "chatPage:error",
      taskId: "task_old",
      requestGeneration: 1,
      message: "Unable to load earlier messages",
    });

    expect(state.chatPages.task_old).toMatchObject({
      pending: false,
      error: "Unable to load earlier messages",
    });
  });

  it("does not let an older page result settle a newer request for the same Task", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_1") });
    state = appReducer(state, {
      type: "chatPage:start",
      taskId: "task_1",
      requestGeneration: 1,
    });
    state = appReducer(state, {
      type: "chatPage:start",
      taskId: "task_1",
      requestGeneration: 2,
    });

    state = appReducer(state, {
      type: "chatPage:result",
      taskId: "task_1",
      requestGeneration: 1,
      page: page("task_1", [chatMessage("old", "Stale page")], false),
    });

    expect(state.chatPages.task_1).toMatchObject({
      requestGeneration: 2,
      pending: true,
      olderItems: [],
    });
  });

  it("does not let an older page error settle a newer request for the same Task", () => {
    let state = createInitialState();
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_1") });
    state = appReducer(state, {
      type: "chatPage:start",
      taskId: "task_1",
      requestGeneration: 1,
    });
    state = appReducer(state, {
      type: "chatPage:start",
      taskId: "task_1",
      requestGeneration: 2,
    });

    state = appReducer(state, {
      type: "chatPage:error",
      taskId: "task_1",
      requestGeneration: 1,
      message: "Stale page failed",
    });

    expect(state.chatPages.task_1).toMatchObject({
      requestGeneration: 2,
      pending: true,
      error: undefined,
    });
  });

  it("reconciles closed active requests and obsolete paging for background Task snapshots", () => {
    let state = createInitialState();
    const background = snapshot("task_background", [chatMessage("tail", "Old tail")]);
    background.active_requests = [permissionMessage("permission-1"), questionMessage("question-1")];
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: background });
    state = appReducer(state, { type: "chatPage:start", taskId: "task_background", requestGeneration: 1 });
    state = appReducer(state, {
      type: "chatPage:result",
      taskId: "task_background",
      requestGeneration: 1,
      page: page("task_background", [chatMessage("page", "Old page")], false),
    });
    state = appReducer(state, { type: "permission:responding", requestId: "permission-1" });
    state = appReducer(state, { type: "question:responding", requestId: "question-1" });
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: snapshot("task_current") });

    const resolvedPermission = permissionMessage("permission-1");
    resolvedPermission.message = {
      ...resolvedPermission.message,
      state: "resolved",
      decision: "approved",
      selected_option: "allow_once",
    };
    const resolvedQuestion = questionMessage("question-1");
    resolvedQuestion.message = {
      ...resolvedQuestion.message,
      state: "resolved",
      answers: [],
    };
    const reconciled = snapshot(
      "task_background",
      [chatMessage("tail", "Old tail"), resolvedPermission, resolvedQuestion],
      2,
    );
    reconciled.history_sync = { state: "updated", generation: 1 };

    state = appReducer(state, { type: "snapshot", intent: "refresh", snapshot: reconciled });

    expect(state.activeTaskId).toBe("task_current");
    expect(state.taskSnapshots.task_background).toBe(reconciled);
    expect(state.chatPages.task_background).toBeUndefined();
    expect(state.permissionResponses["permission-1"]).toBeUndefined();
    expect(state.questionResponses["question-1"]).toBeUndefined();
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

  it("keeps response state while a request is active and clears it when the request closes", () => {
    let state = createInitialState();
    const permission = permissionMessage("server-request-1");
    const active = snapshot("task_1");
    active.active_requests = [permission];
    state = appReducer(state, { type: "snapshot", intent: "open", snapshot: active });
    state = appReducer(state, { type: "permission:responding", requestId: "server-request-1" });
    expect(state.permissionResponses["server-request-1"]).toEqual({ responding: true });

    state = appReducer(state, {
      type: "snapshot",
      intent: "open",
      snapshot: snapshot("task_1", [], 2),
    });

    expect(state.permissionResponses["server-request-1"]).toBeUndefined();
  });
});

function snapshot(taskId: string, items: ChatMessage[] = [], revision = 1): TaskSnapshot {
  const task = taskSummary(taskId);
  return {
    lifecycle: "visible",
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
    active_requests: [],
    history_sync: { state: "idle", generation: 0 },
    send_capability: { state: "ready" },
    settings_summary: {
      agent_id: "codex",
      isolation: "local",
    },
    revision,
  };
}

function configCatalog(currentValue: string) {
  return {
    agent_id: "codex",
    status: "ready" as const,
    options: [{
      current_value: currentValue,
      id: "fast-mode",
      label: "Fast mode",
      values: [
        { id: "off", label: "Off" },
        { id: "on", label: "On" },
      ],
    }],
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
    message_type: "agent_message",
    message_id: id,
    message: {
      kind: "agent_message",
      id,
      role: "agent",
      parts: [{ kind: "text", text }],
      created_at: "2026-05-17T00:00:00Z",
    },
  };
}

function userMessage(id: string, text: string, attachmentCount = 0): ChatMessage {
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
      attachments: Array.from({ length: attachmentCount }, (_, index) => ({
        kind: "file" as const,
        label: `attachment-${index + 1}`,
      })),
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

function questionMessage(requestId: string): ChatMessage & {
  message: Extract<ChatMessage["message"], { kind: "elicitation" }>;
} {
  return {
    cursor: requestId,
    identity: requestId,
    message_id: requestId,
    message_type: "elicitation",
    message: {
      kind: "elicitation",
      id: requestId,
      request_id: requestId,
      app_server_request_id: requestId,
      prompt: "Choose a scope.",
      state: "pending",
      created_at: "2026-07-10T00:00:00Z",
      fields: [],
    },
  };
}

function thoughtMessage(id: string, text: string): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "agent_message",
    message_id: id,
    message: {
      kind: "agent_message",
      id,
      role: "thought",
      parts: [{ kind: "text", text }],
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
