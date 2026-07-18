import { describe, expect, it, vi } from "vitest";
import { STATE_SUBSCRIBE, STATE_UNSUBSCRIBE } from "@openaide/app-server-client";
import type {
  AppServerEvent,
  BackendConnection,
  EventCursor,
  MessageId,
  ProjectId,
  StateRootId,
  StateSubscribeResult,
  TaskId,
  TaskSummary as ProtocolTaskSummary,
} from "@openaide/app-server-client";
import { startAppServerStateSubscription } from "./appServerStateSubscriptions";

describe("startAppServerStateSubscription", () => {
  it("retries an initial subscription failure until a snapshot is available", async () => {
    vi.useFakeTimers();
    try {
      const dispatch = vi.fn();
      const request = vi.fn(async (method: string) => {
        if (method === STATE_UNSUBSCRIBE) {
          return { scope: { kind: "task", taskId: "task_1" as TaskId } };
        }
        if (request.mock.calls.length === 1) throw new Error("temporary disconnect");
        return taskSubscription("cursor_1", "task_1");
      });
      const connection = {
        request,
        handleNotification() {
          return vi.fn();
        },
      } as Pick<BackendConnection, "request" | "handleNotification">;

      const stop = startAppServerStateSubscription({
        backendConnection: connection,
        context: {
          stateRootId: "root_1" as StateRootId,
          clientInstanceId: "client_1" as never,
        },
        dispatch,
        scope: { kind: "task", taskId: "task_1" as TaskId },
      });
      await Promise.resolve();

      await vi.advanceTimersByTimeAsync(500);

      expect(request).toHaveBeenCalledTimes(2);
      expect(dispatch).toHaveBeenCalledOnce();
      stop();
    } finally {
      vi.useRealTimers();
    }
  });

  it("backs off repeated subscription failures", async () => {
    vi.useFakeTimers();
    try {
      const request = vi.fn(async () => {
        if (request.mock.calls.length <= 2) throw new Error("temporary disconnect");
        return taskSubscription("cursor_1", "task_1");
      });
      const connection = {
        request,
        handleNotification() {
          return vi.fn();
        },
      } as Pick<BackendConnection, "request" | "handleNotification">;

      startAppServerStateSubscription({
        backendConnection: connection,
        context: {
          stateRootId: "root_1" as StateRootId,
          clientInstanceId: "client_1" as never,
        },
        dispatch: vi.fn(),
        scope: { kind: "task", taskId: "task_1" as TaskId },
      });
      await Promise.resolve();

      await vi.advanceTimersByTimeAsync(500);
      expect(request).toHaveBeenCalledTimes(2);
      await vi.advanceTimersByTimeAsync(999);
      expect(request).toHaveBeenCalledTimes(2);
      await vi.advanceTimersByTimeAsync(1);
      expect(request).toHaveBeenCalledTimes(3);
    } finally {
      vi.useRealTimers();
    }
  });

  it("dispatches presentation signals for ordered live Agent text deltas", async () => {
    let eventListener: ((event: AppServerEvent) => void) | undefined;
    const dispatch = vi.fn();
    const connection = {
      request: vi.fn(async () => taskSubscription("cursor_1", "task_1")),
      handleNotification(_method: "app/event", listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;
    startAppServerStateSubscription({
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
      },
      dispatch,
      scope: { kind: "task", taskId: "task_1" as TaskId },
    });
    await Promise.resolve();

    const scope = {
      kind: "task" as const,
      stateRootId: "root_1" as StateRootId,
      taskId: "task_1" as TaskId,
    };
    eventListener?.({
      subscription: { kind: "task", taskId: "task_1" as TaskId },
      previousCursor: "cursor_1" as EventCursor,
      cursor: "cursor_2" as EventCursor,
      scope,
      payload: {
        kind: "taskChanged",
        taskId: "task_1" as TaskId,
        revision: 2,
        changes: { chat: [{ kind: "append", item: {
          messageId: "message_1" as MessageId,
          role: "agent",
          status: "complete",
          parts: [{ kind: "text", text: "One" }],
        } }] },
      },
    });
    eventListener?.(textChunkEvent("cursor_2", "cursor_3", " two"));

    const snapshots = dispatch.mock.calls
      .map(([action]) => action)
      .filter((action) => action.type === "snapshot");
    expect(snapshots).toHaveLength(3);
    expect(snapshots[1].snapshot.chat.items[0].message).toMatchObject({
      kind: "agent_message",
      role: "agent",
      parts: [{ kind: "text", text: "One" }],
    });
    expect(snapshots[2].snapshot.chat.items[0].message).toMatchObject({
      kind: "agent_message",
      role: "agent",
      parts: [{ kind: "text", text: "One two" }],
    });
    expect(snapshots[1]).toMatchObject({
      liveText: {
        messageId: "message_1",
        channel: "agent",
        eventCursor: "cursor_2",
      },
    });
    expect(snapshots[2]).toMatchObject({
      liveText: {
        messageId: "message_1",
        channel: "agent",
        eventCursor: "cursor_3",
      },
    });
    expect(dispatch.mock.calls.map(([action]) => action).filter((action) => action.type === "taskChat:liveText")).toEqual([]);
  });

  it("shows a permission card when the task stream publishes an opened request", async () => {
    let eventListener: ((event: AppServerEvent) => void) | undefined;
    const dispatch = vi.fn();
    const connection = {
      request: vi.fn(async () => taskSubscription("cursor_1", "task_1")),
      handleNotification(_method: "app/event", listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;
    startAppServerStateSubscription({
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
      },
      dispatch,
      scope: { kind: "task", taskId: "task_1" as TaskId },
    });
    await Promise.resolve();

    eventListener?.({
      subscription: { kind: "task", taskId: "task_1" as TaskId },
      previousCursor: "cursor_1" as EventCursor,
      cursor: "cursor_2" as EventCursor,
      scope: {
        kind: "task",
        stateRootId: "root_1" as StateRootId,
        taskId: "task_1" as TaskId,
      },
      payload: {
        kind: "taskRequestsUpdated",
        taskId: "task_1" as TaskId,
        requests: [{
          requestId: "server-request-1" as never,
          scope: { kind: "task", taskId: "task_1" as TaskId },
          kind: "permission",
          title: "Run command?",
          permission: {
            title: "Run command?",
            toolCall: { id: "exec-1", title: "Run command", kind: "execute" },
            options: [{
              optionId: "allow-once",
              name: "Allow once",
              kind: "allowOnce",
            }],
          },
        }],
      },
    });

    expect(dispatch).toHaveBeenLastCalledWith(expect.objectContaining({
      type: "snapshot",
      snapshot: expect.objectContaining({
        active_requests: expect.arrayContaining([expect.objectContaining({
          message_type: "permission",
          message: expect.objectContaining({
            app_server_request_id: "server-request-1",
            state: "pending",
          }),
        })]),
      }),
    }));
  });

  it("maps ordered App Server task navigation events into frontend task state", async () => {
    let eventListener: ((event: AppServerEvent) => void) | undefined;
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      expect(method).toBe(STATE_SUBSCRIBE);
      return taskNavigationSubscription("cursor_1", []);
    });
    const connection = {
      request,
      handleNotification(_method: "app/event", listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;

    startAppServerStateSubscription({
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
        agents: [{ agentId: "codex" as never, label: "Codex", status: "connected" }],
        projects: [{ projectId: "project_1" as ProjectId, label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE", available: true }],
      },
      dispatch,
      scope: { kind: "taskNavigation" },
    });
    await Promise.resolve();

    eventListener?.({
      subscription: { kind: "taskNavigation" },
      previousCursor: "cursor_1" as EventCursor,
      cursor: "cursor_2" as EventCursor,
      scope: { kind: "stateRoot", stateRootId: "root_1" as StateRootId },
      payload: {
        kind: "taskNavigationChanged",
        change: { kind: "upsert", task: {
            taskId: "task_1" as TaskId,
            projectId: "project_1" as ProjectId,
            agentId: "codex" as never,
            title: { value: "Real task", source: "user" },
            status: "idle",
            updatedAt: "2026-06-28T00:00:00.000Z",
            lastActivity: "2026-06-28T00:00:00.000Z",
            unread: false,
            hasMessages: true,
            workspaceAvailable: true,
          } },
      },
    });

    expect(dispatch).toHaveBeenLastCalledWith({
      type: "tasks",
      archived: false,
      tasks: [expect.objectContaining({
        task_id: "task_1",
        project_id: "project_1",
        project_label: "OpenAIDE",
        title: "Real task",
      })],
    });
  });

  it("remaps task labels when project metadata arrives after task navigation", async () => {
    const projects = deferred<StateSubscribeResult>();
    const dispatch = vi.fn();
    const context = {
      stateRootId: "root_1" as StateRootId,
      clientInstanceId: "client_1" as never,
      agents: [{ agentId: "codex" as never, label: "Codex", status: "connected" as const }],
    };
    const request = vi.fn((method: string, params?: { scope?: { kind: string } }) => {
      expect(method).toBe(STATE_SUBSCRIBE);
      if (params?.scope?.kind === "taskNavigation") {
        return Promise.resolve(taskNavigationSubscription("cursor_navigation", [
          taskSummary("task_1", "Task without project metadata"),
        ]));
      }
      return projects.promise;
    });
    const connection = {
      request,
      handleNotification() {
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;

    startAppServerStateSubscription({
      backendConnection: connection,
      context,
      dispatch,
      scope: { kind: "taskNavigation" },
    });
    await vi.waitFor(() => expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: "tasks" })));
    expect(dispatch.mock.calls.at(-1)?.[0].tasks[0].project_label).toBeUndefined();

    startAppServerStateSubscription({
      backendConnection: connection,
      context,
      dispatch,
      scope: { kind: "projects" },
    });
    projects.resolve({
      cursor: "cursor_projects" as EventCursor,
      scope: { kind: "projects" },
      snapshot: {
        kind: "projects",
        projects: {
          projects: [{ projectId: "project_1" as ProjectId, label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE", available: true }],
        },
      },
    });
    await projects.promise;
    await vi.waitFor(() => expect(dispatch.mock.calls.at(-1)?.[0]).toMatchObject({
      type: "tasks",
      tasks: [{ task_id: "task_1", project_label: "OpenAIDE" }],
    }));
  });

  it("maps App Server agent subscription snapshots into frontend agent choices", async () => {
    const dispatch = vi.fn();
    const setAgents = vi.fn();
    const request = vi.fn(async (method: string) => {
      expect(method).toBe(STATE_SUBSCRIBE);
      return {
        cursor: "cursor_1" as EventCursor,
        scope: { kind: "agents" as const },
        snapshot: {
          kind: "agents" as const,
          agents: {
            agents: [
              { agentId: "codex" as never, label: "Codex", status: "connected" as const },
              { agentId: "opencode" as never, label: "OpenCode", status: "connected" as const },
            ],
          },
        },
      };
    });
    const connection = {
      request,
      handleNotification() {
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;

    startAppServerStateSubscription({
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
      },
      currentAgentId: () => "",
      dispatch,
      scope: { kind: "agents" },
      setAgents,
    });
    await Promise.resolve();

    expect(setAgents).toHaveBeenCalledWith([
      expect.objectContaining({ id: "codex", label: "Codex" }),
      expect.objectContaining({ id: "opencode", label: "OpenCode" }),
    ]);
    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:agent",
      agentId: "codex",
      agentLabel: "Codex",
    });
  });

  it("applies task navigation events that arrive before the subscribe response resolves", async () => {
    let eventListener: ((event: AppServerEvent) => void) | undefined;
    const dispatch = vi.fn();
    const subscribe = deferred<StateSubscribeResult>();
    const request = vi.fn(async (method: string) => {
      expect(method).toBe(STATE_SUBSCRIBE);
      return subscribe.promise;
    });
    const connection = {
      request,
      handleNotification(_method: "app/event", listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;

    startAppServerStateSubscription({
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
        agents: [{ agentId: "codex" as never, label: "Codex", status: "connected" }],
        projects: [{ projectId: "project_1" as ProjectId, label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE", available: true }],
      },
      dispatch,
      scope: { kind: "taskNavigation" },
    });
    await Promise.resolve();

    eventListener?.({
      subscription: { kind: "taskNavigation" },
      previousCursor: "cursor_1" as EventCursor,
      cursor: "cursor_2" as EventCursor,
      scope: { kind: "stateRoot", stateRootId: "root_1" as StateRootId },
      payload: {
        kind: "taskNavigationChanged",
        change: { kind: "upsert", task: taskSummary("task_1", "Finished task", "completed") },
      },
    });
    subscribe.resolve(taskNavigationSubscription("cursor_1", []));
    await subscribe.promise;
    await Promise.resolve();

    expect(dispatch).toHaveBeenLastCalledWith({
      type: "tasks",
      archived: false,
      tasks: [expect.objectContaining({
        task_id: "task_1",
        title: "Finished task",
        status: "completed",
      })],
    });
  });

  it("retains the remaining queued events when replay discovers a cursor gap", async () => {
    let eventListener: ((event: AppServerEvent) => void) | undefined;
    const dispatch = vi.fn();
    const firstSubscribe = deferred<StateSubscribeResult>();
    const secondSubscribe = deferred<StateSubscribeResult>();
    const request = vi.fn(async () => (
      request.mock.calls.length === 1 ? firstSubscribe.promise : secondSubscribe.promise
    ));
    const connection = {
      request,
      handleNotification(_method: "app/event", listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;

    startAppServerStateSubscription({
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
      },
      dispatch,
      scope: { kind: "task", taskId: "task_1" as TaskId },
    });
    await Promise.resolve();

    eventListener?.(taskEvent("cursor_missing", "cursor_2", "task_1"));
    eventListener?.(taskEvent("cursor_2", "cursor_3", "task_1"));
    firstSubscribe.resolve(taskSubscription("cursor_1", "task_1"));
    await firstSubscribe.promise;
    await Promise.resolve();

    secondSubscribe.resolve(taskSubscription("cursor_2", "task_1"));
    await secondSubscribe.promise;
    await Promise.resolve();

    expect(request).toHaveBeenCalledTimes(2);
    await vi.waitFor(() => expect(dispatch).toHaveBeenCalledTimes(3));
  });

  it("does not replay startup events already included by the subscribe cursor", async () => {
    let eventListener: ((event: AppServerEvent) => void) | undefined;
    const dispatch = vi.fn();
    const subscribe = deferred<StateSubscribeResult>();
    const request = vi.fn(async (method: string) => {
      expect(method).toBe(STATE_SUBSCRIBE);
      return subscribe.promise;
    });
    const connection = {
      request,
      handleNotification(_method: "app/event", listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;

    startAppServerStateSubscription({
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
      },
      dispatch,
      scope: { kind: "taskNavigation" },
    });
    await Promise.resolve();

    eventListener?.({
      subscription: { kind: "taskNavigation" },
      previousCursor: "cursor_1" as EventCursor,
      cursor: "cursor_2" as EventCursor,
      scope: { kind: "stateRoot", stateRootId: "root_1" as StateRootId },
      payload: { kind: "taskNavigationChanged", change: { kind: "upsert", task: taskSummary("task_1", "Older task", "running") } },
    });
    eventListener?.({
      subscription: { kind: "taskNavigation" },
      previousCursor: "cursor_2" as EventCursor,
      cursor: "cursor_3" as EventCursor,
      scope: { kind: "stateRoot", stateRootId: "root_1" as StateRootId },
      payload: { kind: "taskNavigationChanged", change: { kind: "upsert", task: taskSummary("task_1", "Included task", "completed") } },
    });
    subscribe.resolve(taskNavigationSubscription("cursor_3", [taskSummary("task_1", "Included task", "completed")]));
    await subscribe.promise;
    await Promise.resolve();

    expect(request).toHaveBeenCalledTimes(1);
    expect(dispatch).toHaveBeenCalledTimes(1);
    expect(dispatch).toHaveBeenLastCalledWith({
      type: "tasks",
      archived: false,
      tasks: [expect.objectContaining({
        task_id: "task_1",
        title: "Included task",
        status: "completed",
      })],
    });
  });

  it("coalesces repeated resync requests while a subscription refresh is in flight", async () => {
    let eventListener: ((event: AppServerEvent) => void) | undefined;
    const dispatch = vi.fn();
    const secondSubscribe = deferred<StateSubscribeResult>();
    const request = vi.fn(async (method: string) => {
      expect(method).toBe(STATE_SUBSCRIBE);
      if (request.mock.calls.length === 1) return taskSubscription("cursor_1", "task_1");
      return secondSubscribe.promise;
    });
    const connection = {
      request,
      handleNotification(_method: "app/event", listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;

    startAppServerStateSubscription({
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
      },
      dispatch,
      scope: { kind: "task", taskId: "task_1" as TaskId },
    });
    await Promise.resolve();

    eventListener?.(taskEvent("cursor_gap", "cursor_2", "task_1"));
    eventListener?.(taskEvent("cursor_gap", "cursor_3", "task_1"));
    eventListener?.(taskEvent("cursor_gap", "cursor_4", "task_1"));
    await Promise.resolve();

    expect(request).toHaveBeenCalledTimes(2);
    secondSubscribe.resolve(taskSubscription("cursor_4", "task_1"));
    await secondSubscribe.promise;
  });

  it("replays events that arrive while a resync snapshot is in flight", async () => {
    let eventListener: ((event: AppServerEvent) => void) | undefined;
    const dispatch = vi.fn();
    const secondSubscribe = deferred<StateSubscribeResult>();
    const request = vi.fn(async () => (
      request.mock.calls.length === 1
        ? taskSubscription("cursor_1", "task_1")
        : secondSubscribe.promise
    ));
    const connection = {
      request,
      handleNotification(_method: "app/event", listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;

    startAppServerStateSubscription({
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
      },
      dispatch,
      scope: { kind: "task", taskId: "task_1" as TaskId },
    });
    await Promise.resolve();

    eventListener?.(taskEvent("cursor_gap", "cursor_3", "task_1"));
    await Promise.resolve();
    eventListener?.(taskEvent("cursor_4", "cursor_5", "task_1"));

    secondSubscribe.resolve(taskSubscription("cursor_4", "task_1"));
    await secondSubscribe.promise;
    await Promise.resolve();

    expect(request).toHaveBeenCalledTimes(2);
    expect(dispatch).toHaveBeenCalledTimes(3);
  });

  it("keeps events buffered while a failed resync waits to retry", async () => {
    vi.useFakeTimers();
    try {
      let eventListener: ((event: AppServerEvent) => void) | undefined;
      const dispatch = vi.fn();
      const request = vi.fn(async () => {
        if (request.mock.calls.length === 1) return taskSubscription("cursor_1", "task_1");
        if (request.mock.calls.length === 2) throw new Error("temporary disconnect");
        return taskSubscription("cursor_2", "task_1");
      });
      const connection = {
        request,
        handleNotification(_method: "app/event", listener: (event: AppServerEvent) => void) {
          eventListener = listener;
          return vi.fn();
        },
      } as Pick<BackendConnection, "request" | "handleNotification">;

      startAppServerStateSubscription({
        backendConnection: connection,
        context: {
          stateRootId: "root_1" as StateRootId,
          clientInstanceId: "client_1" as never,
        },
        dispatch,
        scope: { kind: "task", taskId: "task_1" as TaskId },
      });
      await Promise.resolve();

      eventListener?.(taskEvent("cursor_gap", "cursor_gap_2", "task_1"));
      await Promise.resolve();
      await Promise.resolve();
      eventListener?.(taskEvent("cursor_1", "cursor_2", "task_1"));

      expect(dispatch).toHaveBeenCalledTimes(1);

      await vi.advanceTimersByTimeAsync(500);

      expect(request).toHaveBeenCalledTimes(3);
      expect(dispatch).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it("ignores another subscription without advancing the Task cursor", async () => {
    let eventListener: ((event: AppServerEvent) => void) | undefined;
    const dispatch = vi.fn();
    const request = vi.fn(async () => taskSubscription("cursor_1", "task_1"));
    const connection = {
      request,
      handleNotification(_method: "app/event", listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;

    startAppServerStateSubscription({
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
      },
      dispatch,
      scope: { kind: "task", taskId: "task_1" as TaskId },
    });
    await Promise.resolve();

    eventListener?.({
      subscription: { kind: "taskNavigation" },
      previousCursor: "cursor_90" as EventCursor,
      cursor: "cursor_91" as EventCursor,
      scope: { kind: "stateRoot", stateRootId: "root_1" as StateRootId },
      payload: { kind: "taskNavigationChanged", change: { kind: "upsert", task: taskSummary("task_2", "Other task") } },
    });
    eventListener?.(taskEvent("cursor_1", "cursor_2", "task_1"));
    await Promise.resolve();

    expect(request).toHaveBeenCalledTimes(1);
    expect(dispatch).toHaveBeenCalledTimes(2);
  });

  it("replays queued Task events independently from equal cursors in another subscription", async () => {
    let eventListener: ((event: AppServerEvent) => void) | undefined;
    const dispatch = vi.fn();
    const subscribe = deferred<StateSubscribeResult>();
    const request = vi.fn(async () => subscribe.promise);
    const connection = {
      request,
      handleNotification(_method: "app/event", listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;

    startAppServerStateSubscription({
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
      },
      dispatch,
      scope: { kind: "task", taskId: "task_1" as TaskId },
    });
    await Promise.resolve();

    eventListener?.(taskEvent("cursor_1", "cursor_2", "task_1"));
    eventListener?.({
      subscription: { kind: "taskNavigation" },
      previousCursor: "navigation_0" as EventCursor,
      cursor: "cursor_1" as EventCursor,
      scope: { kind: "stateRoot", stateRootId: "root_1" as StateRootId },
      payload: {
        kind: "taskNavigationChanged",
        change: { kind: "upsert", task: taskSummary("task_2", "Other task") },
      },
    });
    subscribe.resolve(taskSubscription("cursor_1", "task_1"));
    await subscribe.promise;
    await Promise.resolve();

    expect(dispatch).toHaveBeenCalledTimes(2);
    expect(dispatch).toHaveBeenLastCalledWith(expect.objectContaining({
      type: "snapshot",
      snapshot: expect.objectContaining({ revision: 2 }),
    }));
  });

  it("unsubscribes the App Server scope when disposed", async () => {
    const stopEvents = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === STATE_SUBSCRIBE) return taskSubscription("cursor_1", "task_1");
      if (method === STATE_UNSUBSCRIBE) return { scope: { kind: "task", taskId: "task_1" as TaskId } };
      throw new Error(`unexpected method ${method}`);
    });
    const connection = {
      request,
      handleNotification() {
        return stopEvents;
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;

    const stop = startAppServerStateSubscription({
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
      },
      dispatch: vi.fn(),
      scope: { kind: "task", taskId: "task_1" as TaskId },
    });
    await Promise.resolve();
    stop();
    await Promise.resolve();

    expect(stopEvents).toHaveBeenCalledOnce();
    expect(request).toHaveBeenCalledWith(STATE_UNSUBSCRIBE, { scope: { kind: "task", taskId: "task_1" } });
  });

  it("cleans up a subscription that resolves after the view was disposed", async () => {
    const subscribe = deferred<void>();
    let serverSubscribed = false;
    const request = vi.fn(async (method: string) => {
      if (method === STATE_SUBSCRIBE) {
        await subscribe.promise;
        serverSubscribed = true;
        return taskSubscription("cursor_1", "task_1");
      }
      if (method === STATE_UNSUBSCRIBE) {
        serverSubscribed = false;
        return { scope: { kind: "task", taskId: "task_1" as TaskId } };
      }
      throw new Error(`unexpected method ${method}`);
    });
    const connection = {
      request,
      handleNotification() {
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;

    const stop = startAppServerStateSubscription({
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
      },
      dispatch: vi.fn(),
      scope: { kind: "task", taskId: "task_1" as TaskId },
    });
    await Promise.resolve();
    stop();

    subscribe.resolve();
    await subscribe.promise;
    await vi.waitFor(() => expect(serverSubscribed).toBe(false));
  });

  it("does not let late cleanup remove a successor watching the same scope", async () => {
    const firstSubscribe = deferred<void>();
    let subscribeCount = 0;
    let serverSubscribed = false;
    const request = vi.fn(async (method: string) => {
      if (method === STATE_SUBSCRIBE) {
        subscribeCount += 1;
        if (subscribeCount === 1) await firstSubscribe.promise;
        serverSubscribed = true;
        return taskSubscription(`cursor_${subscribeCount}`, "task_1");
      }
      if (method === STATE_UNSUBSCRIBE) {
        serverSubscribed = false;
        return { scope: { kind: "task", taskId: "task_1" as TaskId } };
      }
      throw new Error(`unexpected method ${method}`);
    });
    const connection = {
      request,
      handleNotification() {
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;
    const options = {
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
      },
      dispatch: vi.fn(),
      scope: { kind: "task" as const, taskId: "task_1" as TaskId },
    };

    const stopFirst = startAppServerStateSubscription(options);
    await Promise.resolve();
    stopFirst();
    const stopSecond = startAppServerStateSubscription(options);
    await vi.waitFor(() => expect(serverSubscribed).toBe(true));

    firstSubscribe.resolve();
    await firstSubscribe.promise;
    await vi.waitFor(() => expect(serverSubscribed).toBe(true));
    stopSecond();
    await vi.waitFor(() => expect(serverSubscribed).toBe(false));
  });

  it("orders successor subscribe after an older scope cleanup", async () => {
    const cleanup = deferred<void>();
    let serverSubscribed = false;
    const request = vi.fn(async (method: string) => {
      if (method === STATE_SUBSCRIBE) {
        serverSubscribed = true;
        return taskSubscription("cursor_1", "task_1");
      }
      if (method === STATE_UNSUBSCRIBE) {
        await cleanup.promise;
        serverSubscribed = false;
        return { scope: { kind: "task", taskId: "task_1" as TaskId } };
      }
      throw new Error(`unexpected method ${method}`);
    });
    const connection = {
      request,
      handleNotification() {
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "handleNotification">;
    const options = {
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
      },
      dispatch: vi.fn(),
      scope: { kind: "task" as const, taskId: "task_1" as TaskId },
    };

    const stopFirst = startAppServerStateSubscription(options);
    await Promise.resolve();
    stopFirst();
    const stopSecond = startAppServerStateSubscription(options);
    await Promise.resolve();

    cleanup.resolve();
    await cleanup.promise;
    await vi.waitFor(() => expect(serverSubscribed).toBe(true));
    stopSecond();
  });
});

function taskNavigationSubscription(
  cursor: string,
  tasks: ProtocolTaskSummary[],
): StateSubscribeResult {
  return {
    cursor: cursor as EventCursor,
    scope: { kind: "taskNavigation" },
    snapshot: {
      kind: "taskNavigation",
      navigation: { tasks },
    },
  };
}

function taskNavigationEvent(previousCursor: string, cursor: string): AppServerEvent {
  return {
    subscription: { kind: "taskNavigation" },
    previousCursor: previousCursor as EventCursor,
    cursor: cursor as EventCursor,
    scope: { kind: "stateRoot", stateRootId: "root_1" as StateRootId },
    payload: { kind: "taskNavigationChanged", change: { kind: "remove", taskId: "absent" as TaskId } },
  };
}

function taskSubscription(cursor: string, taskIdValue: string): StateSubscribeResult {
  return {
    cursor: cursor as EventCursor,
    scope: { kind: "task", taskId: taskIdValue as TaskId },
    snapshot: {
      kind: "task",
      task: protocolTaskSnapshot(taskIdValue),
    },
  };
}

function taskEvent(
  previousCursor: string,
  cursor: string,
  taskIdValue: string,
  stateRootId = "root_1",
  revision = 2,
): AppServerEvent {
  return {
    subscription: { kind: "task", taskId: taskIdValue as TaskId },
    previousCursor: previousCursor as EventCursor,
    cursor: cursor as EventCursor,
    scope: {
      kind: "task",
      stateRootId: stateRootId as StateRootId,
      taskId: taskIdValue as TaskId,
    },
    payload: {
      kind: "taskChanged",
      taskId: taskIdValue as TaskId,
      revision,
      changes: { task: taskSummary(taskIdValue, "Task") },
    },
  };
}

function textChunkEvent(
  previousCursor: string,
  cursor: string,
  text: string,
): AppServerEvent {
  return {
    subscription: { kind: "task", taskId: "task_1" as TaskId },
    previousCursor: previousCursor as EventCursor,
    cursor: cursor as EventCursor,
    scope: {
      kind: "task",
      stateRootId: "root_1" as StateRootId,
      taskId: "task_1" as TaskId,
    },
    payload: {
      kind: "taskChanged",
      taskId: "task_1" as TaskId,
      revision: Number(cursor.slice(-1)),
      changes: { chat: [{ kind: "appendText", messageId: "message_1" as MessageId, text }] },
    },
  };
}

function protocolTaskSnapshot(taskIdValue: string) {
  return {
    task: taskSummary(taskIdValue, "Task"),
    lifecycle: "visible" as const,
    revision: 1,
    preparation: { kind: "ready" as const },
    agentConfig: { state: "ready" as const, options: [] },
    agentCommands: { state: "ready" as const, commands: [] },
    sendCapability: { state: "ready" as const },
    historySync: { state: "idle" as const, generation: 0 },
    chat: { items: [], hasMessages: false },
    pendingRequests: [],
  };
}

function taskSummary(taskIdValue: string, title: string, status: ProtocolTaskSummary["status"] = "idle"): ProtocolTaskSummary {
  return {
    taskId: taskIdValue as TaskId,
    projectId: "project_1" as ProjectId,
    agentId: "codex" as never,
    title: { value: title, source: "user" },
    status,
    updatedAt: "2026-06-28T00:00:00.000Z",
    lastActivity: "2026-06-28T00:00:00.000Z",
    unread: false,
    hasMessages: true,
    workspaceAvailable: true,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}
