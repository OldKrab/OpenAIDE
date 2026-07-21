import { describe, expect, it, vi } from "vitest";
import type {
  AppServerSession,
  AppServerStateObserver,
  StateRootId,
  SubscriptionScope,
} from "@openaide/app-server-client";
import { startAppServerStateSubscription } from "./appServerStateSubscriptions";

describe("startAppServerStateSubscription", () => {
  it("maps a session-owned Projects baseline into Frontend state", () => {
    const subscription = fakeSubscription();
    const dispatch = vi.fn();

    startAppServerStateSubscription({
      backendConnection: subscription.connection,
      context: { stateRootId: "root_1" as StateRootId },
      dispatch,
      scope: { kind: "projects" },
    });
    subscription.observer().onSnapshot({
      kind: "projects",
      projects: {
        projects: [{
          projectId: "project_1" as never,
          label: "OpenAIDE",
          workspaceRoot: "/workspace/OpenAIDE",
          available: true,
          worktreeRepositoryId: "repository_1" as never,
          projectWorktreeId: "worktree_root" as never,
          worktreeError: null,
        }],
      },
    });

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
  });

  it("maps a session-owned Worktree Repository baseline into Frontend state", () => {
    const subscription = fakeSubscription();
    const dispatch = vi.fn();
    const repository = {
      repositoryId: "repository_1",
      revision: 3,
      worktrees: [],
    };

    startAppServerStateSubscription({
      backendConnection: subscription.connection,
      context: { stateRootId: "root_1" as StateRootId },
      dispatch,
      scope: { kind: "worktreeRepository", repositoryId: "repository_1" as never },
    });
    subscription.observer().onSnapshot({
      kind: "worktreeRepository",
      repository,
    } as never);

    expect(dispatch).toHaveBeenCalledWith({ type: "worktreeRepository", repository });
  });

  it("remaps task navigation when session-owned Project metadata arrives", () => {
    const navigation = fakeSubscription();
    const projects = fakeSubscription();
    const dispatch = vi.fn();
    const context = {
      stateRootId: "root_1" as StateRootId,
      agents: [{ agentId: "codex", label: "Codex", status: "connected" }] as never,
    };
    startAppServerStateSubscription({
      backendConnection: navigation.connection,
      context,
      dispatch,
      scope: { kind: "taskNavigation" },
    });
    startAppServerStateSubscription({
      backendConnection: projects.connection,
      context,
      dispatch,
      scope: { kind: "projects" },
    });

    navigation.observer().onSnapshot({
      kind: "taskNavigation",
      navigation: { entries: [{ kind: "task", task: {
        taskId: "task_1",
        projectId: "project_1",
        agentId: "codex",
        title: { value: "Recovered Task", source: "user" },
        status: "idle",
        updatedAt: "2026-07-18T00:00:00.000Z",
        lastActivity: "2026-07-18T00:00:00.000Z",
        unread: false,
        hasMessages: true,
      } }] },
    } as never);
    projects.observer().onSnapshot({
      kind: "projects",
      projects: { projects: [{ projectId: "project_1", label: "OpenAIDE" }] },
    } as never);

    expect(dispatch).toHaveBeenLastCalledWith({
      type: "taskNavigation",
      archived: false,
      refreshing: false,
      sessions: [],
      tasks: [expect.objectContaining({
        task_id: "task_1",
        project_label: "OpenAIDE",
        title: "Recovered Task",
      })],
    });
  });

  it("maps persisted unadopted sessions from Task Navigation into the shared sidebar state", () => {
    const navigation = fakeSubscription();
    const dispatch = vi.fn();
    startAppServerStateSubscription({
      backendConnection: navigation.connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        agents: [{ agentId: "codex", label: "Codex", status: "connected" }] as never,
        projects: [{ projectId: "project_1", label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE" }] as never,
      },
      dispatch,
      scope: { kind: "taskNavigation" },
    });

    navigation.observer().onSnapshot({
      kind: "taskNavigation",
      navigation: {
        refreshing: true,
        entries: [{
          kind: "nativeSession",
          session: {
            reference: { agentId: "codex", sessionId: "session_1" },
            projectId: "project_1",
            workspaceRoot: "/workspace/OpenAIDE",
            title: "Persisted session",
            lastActivity: "2026-07-21T00:00:00Z",
          },
        }],
      },
    } as never);

    expect(dispatch).toHaveBeenLastCalledWith({
      type: "taskNavigation",
      archived: false,
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

  it("maps Agent baselines and ordered live text without owning stream state", () => {
    const agents = fakeSubscription();
    const task = fakeSubscription();
    const dispatch = vi.fn();
    const setAgents = vi.fn();
    startAppServerStateSubscription({
      backendConnection: agents.connection,
      context: { stateRootId: "root_1" as StateRootId },
      currentAgentId: () => "",
      dispatch,
      scope: { kind: "agents" },
      setAgents,
    });
    startAppServerStateSubscription({
      backendConnection: task.connection,
      context: { stateRootId: "root_1" as StateRootId },
      dispatch,
      scope: { kind: "task", taskId: "task_1" as never },
    });
    agents.observer().onSnapshot({
      kind: "agents",
      agents: { agents: [{ agentId: "codex", label: "Codex", status: "connected" }] },
    } as never);
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
      cursor: "cursor_2",
      payload: {
        kind: "taskChanged",
        taskId: "task_1",
        changes: { chat: [{ kind: "appendText", messageId: "message_1", text: " world" }] },
      },
    } as never, false);

    expect(setAgents).toHaveBeenCalledWith([
      expect.objectContaining({ id: "codex", label: "Codex" }),
    ]);
    expect(dispatch).toHaveBeenLastCalledWith({
      type: "taskChat:liveText",
      taskId: "task_1",
      messageId: "message_1",
      channel: "agent",
      eventCursor: "cursor_2",
    });
  });

  it("forwards scope readiness from the session module", () => {
    const subscription = fakeSubscription();
    const onBaselineError = vi.fn();
    const onBaselineLost = vi.fn();
    const onBaselineReady = vi.fn();

    const stop = startAppServerStateSubscription({
      backendConnection: subscription.connection,
      context: { stateRootId: "root_1" as StateRootId },
      dispatch: vi.fn(),
      onBaselineError,
      onBaselineLost,
      onBaselineReady,
      scope: { kind: "agents" },
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
