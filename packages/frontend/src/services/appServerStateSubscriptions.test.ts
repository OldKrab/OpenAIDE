import { describe, expect, it, vi } from "vitest";
import type {
  AppServerSession,
  AppServerStateObserver,
  StateRootId,
  SubscriptionScope,
} from "@openaide/app-server-client";
import type { GlobalStateChanges } from "@openaide/app-server-client";
import { startGlobalSyncSubscription } from "./appServerGlobalSync";
import { startAppServerStateSubscription } from "./appServerStateSubscriptions";

describe("startAppServerStateSubscription", () => {
  it("clears a focused Task when the App Server resets its history", () => {
    const task = fakeSubscription();
    const onTaskRemoved = vi.fn();
    const dispatch = vi.fn();
    startAppServerStateSubscription({
      context: {},
      backendConnection: task.connection,
      onTaskRemoved,
      dispatch,
      scope: { kind: "task", taskId: "task_1" as never },
    });

    task.observer().onSnapshot({ kind: "task", task: {} } as never, {
      payload: {
        kind: "taskChanged",
        taskId: "task_1",
        revision: 2,
        changes: { removed: true },
      },
    } as never);

    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "task:list:remove", taskId: "task_1" });
    expect(onTaskRemoved).toHaveBeenCalledWith("task_1");
    expect(dispatch).toHaveBeenCalledTimes(1);
  });

  it("maps one sync notification into every changed area in one step", () => {
    const sync = fakeSync();
    const dispatch = vi.fn();
    const setAgents = vi.fn();
    const repository = { repositoryId: "repository_1", revision: 3, worktrees: [] };
    const devices = {
      remoteAccess: "on" as const,
      serverName: "workstation",
      devices: [{ deviceId: "device_1", name: "Phone", addedAtMs: 1 }],
    };
    startGlobalSyncSubscription({
      context: {},
      backendConnection: sync.connection,
      currentAgentId: () => "",
      dispatch,
      setAgents,
    });

    sync.emit({
      projects: {
        projects: [{
          projectId: "project_1",
          label: "OpenAIDE",
          workspaceRoot: "/workspace/OpenAIDE",
          available: true,
          worktreeRepositoryId: "repository_1",
          projectWorktreeId: "worktree_root",
          worktreeError: null,
        }],
      },
      agents: { agents: [{ agentId: "codex", label: "Codex", status: "connected" }] },
      worktrees: [repository],
      devices,
    } as never);

    expect(dispatch).toHaveBeenCalledWith({
      type: "projects",
      projects: [{
        projectId: "project_1",
        label: "OpenAIDE",
        workspaceRoot: "/workspace/OpenAIDE",
        available: true,
        worktreeRepositoryId: "repository_1",
        projectWorktreeId: "worktree_root",
        worktreeError: undefined,
      }],
    });
    expect(dispatch).toHaveBeenCalledWith({ type: "worktreeRepository", repository });
    expect(dispatch).toHaveBeenCalledWith({ type: "settings:devices", devices });
    expect(setAgents).toHaveBeenCalledWith([expect.objectContaining({ id: "codex", label: "Codex" })]);
  });

  it("applies settings and removed worktree repositories from a sync notification", () => {
    const sync = fakeSync();
    const dispatch = vi.fn();
    const setPreferences = vi.fn();
    startGlobalSyncSubscription({ context: {}, backendConnection: sync.connection, dispatch, setPreferences });

    sync.emit({
      settings: {
        sections: [],
        preferences: { preferences: { composerSubmitShortcut: "enter" } },
        runtime: { developer: { acpTrace: { enabled: true, directory: "trace" } } },
      },
      removedWorktreeRepositories: ["repository_1"],
    } as never);

    expect(dispatch).toHaveBeenCalledWith({
      type: "settings:runtimeSettings",
      settings: { developer: { acp_trace: { enabled: true, directory: "trace" } } },
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "settings:preferences",
      preferences: { composer_submit_shortcut: "enter" },
    });
    expect(setPreferences).toHaveBeenCalledWith({ composer_submit_shortcut: "enter" });
    expect(dispatch).toHaveBeenCalledWith({ type: "worktreeRepository:removed", repositoryId: "repository_1" });
  });

  it("applies New Task defaults only where the user has not chosen", () => {
    const sync = fakeSync();
    const dispatch = vi.fn();
    let live: { projectId?: string; agentId?: string } = {};
    const context = {
      agents: [{ agentId: "codex", label: "Codex", status: "connected" }] as never,
      projects: [{ projectId: "project_1", label: "App", workspaceRoot: "/w/App", available: true }] as never,
    };
    startGlobalSyncSubscription({
      context,
      backendConnection: sync.connection,
      dispatch,
      liveNewTaskContext: () => live,
    });

    sync.emit({ newTaskDefaults: { projectId: "project_1", agentId: "codex" } } as never);

    expect(dispatch).toHaveBeenCalledWith({ type: "newTask:agent", agentId: "codex", agentLabel: "Codex" });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: "projects", initialProjectId: "project_1" }));

    dispatch.mockClear();
    live = { projectId: "project_1", agentId: "codex" };
    sync.emit({ newTaskDefaults: { projectId: "project_1", agentId: "codex" } } as never);
    expect(dispatch).not.toHaveBeenCalled();
  });

  it("dispatches nothing for areas a sync notification leaves out", () => {
    const sync = fakeSync();
    const dispatch = vi.fn();
    startGlobalSyncSubscription({
      context: {},
      backendConnection: sync.connection,
      dispatch,
    });

    sync.emit({} as never);

    expect(dispatch).not.toHaveBeenCalled();
  });

  it("remaps task navigation when session-owned Project metadata arrives", () => {
    const navigation = fakeSubscription();
    const sync = fakeSync();
    const dispatch = vi.fn();
    const context = {
      
      agents: [{ agentId: "codex", label: "Codex", status: "connected" }] as never,
    };
    startAppServerStateSubscription({
      backendConnection: navigation.connection,
      context,
      dispatch,
      scope: { kind: "taskNavigation", section: "tasks" },
      view: viewParams("tasks"),
    });
    startGlobalSyncSubscription({ backendConnection: sync.connection, context, dispatch });

    navigation.observer().onSnapshot({
      kind: "taskNavigation",
      navigation: replica("tasks", [row("task:task_1", { live: false, entry: { kind: "task", task: {
        taskId: "task_1",
        projectId: "project_1",
        agentId: "codex",
        title: { value: "Recovered Task", source: "user" },
        status: "idle",
        updatedAt: "2026-07-18T00:00:00.000Z",
        lastActivity: "2026-07-18T00:00:00.000Z",
        unread: false,
        hasMessages: true,
      } } })]),
    } as never);
    sync.emit({
      projects: { projects: [{ projectId: "project_1", label: "OpenAIDE" }] },
    } as never);

    expect(dispatch).toHaveBeenLastCalledWith({
      type: "taskNavigation",
      archived: false,
      groups: [expect.objectContaining({
        projectId: "project_1",
        label: "OpenAIDE",
        total: 1,
        rows: [expect.objectContaining({
          kind: "task",
          live: false,
          task: expect.objectContaining({
            task_id: "task_1",
            project_label: "OpenAIDE",
            title: "Recovered Task",
          }),
        })],
      })],
      refreshError: undefined,
      refreshing: false,
      sessions: [],
      tasks: [expect.objectContaining({ task_id: "task_1", project_label: "OpenAIDE" })],
    });
  });

  it("passes the Navigation view parameters to the subscription", () => {
    const navigation = fakeSubscription();
    const view = viewParams("tasks");

    startAppServerStateSubscription({
      context: {},
      backendConnection: navigation.connection,
      dispatch: vi.fn(),
      scope: { kind: "taskNavigation", section: "tasks" },
      view,
    });

    expect(navigation.connection.subscribeState).toHaveBeenCalledWith(
      { kind: "taskNavigation", section: "tasks" },
      expect.anything(),
      view,
    );
  });

  it("maps persisted unadopted sessions from Task Navigation into the shared sidebar state", () => {
    const navigation = fakeSubscription();
    const dispatch = vi.fn();
    startAppServerStateSubscription({
      backendConnection: navigation.connection,
      context: {
        agents: [{ agentId: "codex", label: "Codex", status: "connected" }] as never,
        projects: [{ projectId: "project_1", label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE" }] as never,
      },
      dispatch,
      scope: { kind: "taskNavigation", section: "tasks" },
      view: viewParams("tasks"),
    });

    navigation.observer().onSnapshot({
      kind: "taskNavigation",
      navigation: replica("tasks", [row("session:codex:session_1", {
        live: false,
        entry: {
          kind: "nativeSession",
          session: {
            reference: { agentId: "codex", sessionId: "session_1" },
            projectId: "project_1",
            workspaceRoot: "/workspace/OpenAIDE",
            title: "Persisted session",
            lastActivity: "2026-07-21T00:00:00Z",
          },
        },
      })], { discovery: "loading" }),
    } as never);

    expect(dispatch).toHaveBeenLastCalledWith({
      type: "taskNavigation",
      archived: false,
      groups: [expect.objectContaining({ discovery: "loading", rows: [expect.objectContaining({ kind: "session" })] })],
      refreshError: undefined,
      refreshing: true,
      sessions: [{
        agent_id: "codex",
        agent_name: "Codex",
        cwd: "/workspace/OpenAIDE",
        last_activity: "2026-07-21T00:00:00Z",
        project_id: "project_1",
        session_id: "session_1",
        title: "Persisted session",
      }],
      tasks: [],
    });
  });

  it("keeps archived Native Sessions in the Archive section replica", () => {
    const navigation = fakeSubscription();
    const dispatch = vi.fn();
    startAppServerStateSubscription({
      backendConnection: navigation.connection,
      context: {
        agents: [{ agentId: "codex", label: "Codex", status: "connected" }] as never,
      },
      dispatch,
      scope: { kind: "taskNavigation", section: "archive" },
      view: viewParams("archive"),
    });

    navigation.observer().onSnapshot({
      kind: "taskNavigation",
      navigation: replica("archive", [row("session:codex:session_1", {
        live: false,
        entry: {
          kind: "nativeSession",
          session: {
            reference: { agentId: "codex", sessionId: "session_1" },
            projectId: "project_1",
            workspaceRoot: "/workspace/OpenAIDE",
            title: "Archived session",
            lastActivity: null,
          },
        },
      })]),
    } as never);

    expect(dispatch).toHaveBeenLastCalledWith({
      type: "taskNavigation",
      archived: true,
      groups: [expect.objectContaining({ rows: [expect.objectContaining({ kind: "session" })] })],
      refreshError: undefined,
      refreshing: false,
      sessions: [expect.objectContaining({
        agent_id: "codex",
        session_id: "session_1",
        title: "Archived session",
      })],
      tasks: [],
    });
  });

  it("maps ordered live Task text without owning stream state", () => {
    const task = fakeSubscription();
    const dispatch = vi.fn();
    startAppServerStateSubscription({
      context: {},
      backendConnection: task.connection,
      dispatch,
      scope: { kind: "task", taskId: "task_1" as never },
    });
    task.observer().onSnapshot({
      kind: "task",
      task: {
        chat: { items: [{
          messageId: "message_1",
          role: "agent",
          status: "streaming",
          parts: [{ kind: "text", text: "Hello" }],
        }] },
      },
    } as never, {
      payload: {
        kind: "taskChanged",
        taskId: "task_1",
        revision: 2,
        changes: { chat: [{ kind: "appendText", messageId: "message_1", text: " world" }] },
      },
    } as never, false);

    expect(dispatch).toHaveBeenLastCalledWith({
      type: "taskChat:liveText",
      taskId: "task_1",
      messageId: "message_1",
      channel: "agent",
      eventToken: "2",
    });
  });

  it("forwards scope readiness from the session module", () => {
    const subscription = fakeSubscription();
    const onBaselineError = vi.fn();
    const onBaselineLost = vi.fn();
    const onBaselineReady = vi.fn();

    const stop = startAppServerStateSubscription({
      context: {},
      backendConnection: subscription.connection,
      dispatch: vi.fn(),
      onBaselineError,
      onBaselineLost,
      onBaselineReady,
      scope: { kind: "taskNavigation", section: "tasks" },
      view: viewParams("tasks"),
    });
    subscription.observer().onBaselineLost?.();
    subscription.observer().onBaselineError?.(new Error("offline"));
    subscription.observer().onBaselineReady?.();
    stop();

    expect(onBaselineLost).toHaveBeenCalledOnce();
    expect(onBaselineError).toHaveBeenCalledWith(expect.objectContaining({ message: "offline" }));
    expect(onBaselineReady).toHaveBeenCalledOnce();
    expect(subscription.stop).toHaveBeenCalledOnce();
  });
});

function fakeSubscription() {
  let currentObserver: AppServerStateObserver | undefined;
  const stop = vi.fn();
  const subscribeState = vi.fn((
    _scope: SubscriptionScope,
    observer: AppServerStateObserver,
  ) => {
    currentObserver = observer;
    return stop;
  });
  return {
    connection: { subscribeState } as Pick<AppServerSession, "subscribeState">,
    observer() {
      if (!currentObserver) throw new Error("Subscription observer was not installed");
      return currentObserver;
    },
    stop,
  };
}

function fakeSync() {
  let listener: ((changes: GlobalStateChanges) => void) | undefined;
  const handleNotification = vi.fn((_method: string, next: (changes: GlobalStateChanges) => void) => {
    listener = next;
    return () => undefined;
  });
  return {
    connection: { handleNotification } as unknown as Pick<AppServerSession, "handleNotification">,
    emit(changes: GlobalStateChanges) {
      if (!listener) throw new Error("sync listener was not installed");
      listener(changes);
    },
  };
}

function viewParams(section: "tasks" | "archive") {
  return { section, historyLimit: 20, historyLimits: [] };
}

function row(id: string, body: unknown): [string, unknown] {
  return [id, body];
}

/** A Navigation replica holding one Project group whose rows are the given `[id, body]` pairs. */
function replica(
  section: "tasks" | "archive",
  rows: Array<[string, unknown]>,
  overrides: Record<string, unknown> = {},
) {
  return {
    section,
    groups: [{
      projectId: "project_1",
      projectLabel: "OpenAIDE",
      order: rows.map(([id]) => id),
      rowsById: Object.fromEntries(rows),
      total: rows.length,
      hidden: 0,
      undiscovered: false,
      discovery: "settled",
      ...overrides,
    }],
  };
}
