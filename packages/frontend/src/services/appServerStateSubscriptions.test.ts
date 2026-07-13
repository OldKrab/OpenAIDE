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
        events() {
          return vi.fn();
        },
      } as Pick<BackendConnection, "request" | "events">;

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
        events() {
          return vi.fn();
        },
      } as Pick<BackendConnection, "request" | "events">;

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

  it("refreshes its snapshot when the connection loses event continuity", async () => {
    let resetListener: Parameters<BackendConnection["stateResets"]>[0] | undefined;
    const dispatch = vi.fn();
    const request = vi.fn(async () => taskSubscription(
      request.mock.calls.length === 1 ? "cursor_1" : "cursor_5",
      "task_1",
    ));
    const connection = {
      request,
      events() {
        return vi.fn();
      },
      stateResets(listener: Parameters<BackendConnection["stateResets"]>[0]) {
        resetListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "events" | "request" | "stateResets">;

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

    resetListener?.({ serverId: "server_1" as never, stateRootId: "root_1" as StateRootId });
    await Promise.resolve();

    expect(request).toHaveBeenCalledTimes(2);
    expect(dispatch).toHaveBeenCalledTimes(2);
  });

  it("does not replay events queued before a replica reset onto its replacement baseline", async () => {
    let eventListener: ((event: AppServerEvent) => void) | undefined;
    let resetListener: Parameters<BackendConnection["stateResets"]>[0] | undefined;
    const firstSubscribe = deferred<StateSubscribeResult>();
    const secondSubscribe = deferred<StateSubscribeResult>();
    const dispatch = vi.fn();
    const request = vi.fn(() => (
      request.mock.calls.length === 1 ? firstSubscribe.promise : secondSubscribe.promise
    ));
    const connection = {
      request,
      events(listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
      stateResets(listener: Parameters<BackendConnection["stateResets"]>[0]) {
        resetListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "events" | "request" | "stateResets">;

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
    resetListener?.({ serverId: "server_2" as never, stateRootId: "root_1" as StateRootId });
    firstSubscribe.resolve(taskSubscription("cursor_1", "task_1"));
    await firstSubscribe.promise;
    await vi.waitFor(() => expect(request).toHaveBeenCalledTimes(2));

    secondSubscribe.resolve(taskSubscription("cursor_10", "task_1"));
    await secondSubscribe.promise;
    await vi.waitFor(() => expect(dispatch).toHaveBeenCalledOnce());
    expect(request).toHaveBeenCalledTimes(2);
  });

  it("uses the replacement state root for its reset baseline and live events", async () => {
    let eventListener: ((event: AppServerEvent) => void) | undefined;
    let resetListener: Parameters<BackendConnection["stateResets"]>[0] | undefined;
    const dispatch = vi.fn();
    const context = {
      stateRootId: "root_1" as StateRootId,
      clientInstanceId: "client_1" as never,
    };
    const request = vi.fn(async () => taskSubscription(
      request.mock.calls.length === 1 ? "cursor_1" : "cursor_10",
      "task_1",
    ));
    const connection = {
      request,
      events(listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
      stateResets(listener: Parameters<BackendConnection["stateResets"]>[0]) {
        resetListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "events" | "request" | "stateResets">;

    startAppServerStateSubscription({
      backendConnection: connection,
      context,
      dispatch,
      scope: { kind: "task", taskId: "task_1" as TaskId },
    });
    await vi.waitFor(() => expect(dispatch).toHaveBeenCalledOnce());

    context.stateRootId = "root_2" as StateRootId;
    resetListener?.({ serverId: "server_2" as never, stateRootId: context.stateRootId });
    await vi.waitFor(() => expect(dispatch).toHaveBeenCalledTimes(2));

    eventListener?.(taskEvent("cursor_10", "cursor_11", "task_1", "root_2"));
    expect(dispatch).toHaveBeenCalledTimes(3);
    eventListener?.(taskEvent("cursor_11", "cursor_12", "task_1", "root_1"));
    expect(dispatch).toHaveBeenCalledTimes(3);
    expect(request).toHaveBeenCalledTimes(3);
  });

  it("dispatches presentation signals for ordered live Agent text deltas", async () => {
    let eventListener: ((event: AppServerEvent) => void) | undefined;
    const dispatch = vi.fn();
    const connection = {
      request: vi.fn(async () => taskSubscription("cursor_1", "task_1")),
      events(listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "events">;
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
      previousCursor: "cursor_1" as EventCursor,
      cursor: "cursor_2" as EventCursor,
      scope,
      payload: {
        kind: "chatItemAppended",
        taskId: "task_1" as TaskId,
        revision: 2,
        item: {
          messageId: "message_1" as MessageId,
          role: "agent",
          status: "complete",
          parts: [{ kind: "text", text: "One" }],
        },
      },
    });
    eventListener?.(textChunkEvent("cursor_2", "cursor_3", " two"));

    const snapshots = dispatch.mock.calls
      .map(([action]) => action)
      .filter((action) => action.type === "snapshot");
    expect(snapshots).toHaveLength(3);
    expect(snapshots[1].snapshot.chat.items[0].message).toMatchObject({ text: "One" });
    expect(snapshots[2].snapshot.chat.items[0].message).toMatchObject({ text: "One two" });
    expect(dispatch.mock.calls.map(([action]) => action).filter((action) => action.type === "taskChat:liveText")).toEqual([
      {
        type: "taskChat:liveText",
        taskId: "task_1",
        messageId: "message_1",
        channel: "agent",
        eventCursor: "cursor_2",
      },
      {
        type: "taskChat:liveText",
        taskId: "task_1",
        messageId: "message_1",
        channel: "agent",
        eventCursor: "cursor_3",
      },
    ]);
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
      events(listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "events">;

    startAppServerStateSubscription({
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
        agents: [{ agentId: "codex" as never, label: "Codex", status: "connected" }],
        projects: [{ projectId: "project_1" as ProjectId, label: "OpenAIDE" }],
      },
      dispatch,
      scope: { kind: "taskNavigation" },
    });
    await Promise.resolve();

    eventListener?.({
      previousCursor: "cursor_1" as EventCursor,
      cursor: "cursor_2" as EventCursor,
      scope: { kind: "stateRoot", stateRootId: "root_1" as StateRootId },
      payload: {
        kind: "taskNavigationUpdated",
        navigation: {
          tasks: [{
            taskId: "task_1" as TaskId,
            projectId: "project_1" as ProjectId,
            agentId: "codex" as never,
            title: { value: "Real task", source: "user" },
            status: "idle",
            updatedAt: "2026-06-28T00:00:00.000Z",
            lastActivity: "2026-06-28T00:00:00.000Z",
            unread: false,
            hasMessages: true,
          }],
        },
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
      events() {
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "events">;

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
          projects: [{ projectId: "project_1" as ProjectId, label: "OpenAIDE" }],
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
      events() {
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "events">;

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
      events(listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "events">;

    startAppServerStateSubscription({
      backendConnection: connection,
      context: {
        stateRootId: "root_1" as StateRootId,
        clientInstanceId: "client_1" as never,
        agents: [{ agentId: "codex" as never, label: "Codex", status: "connected" }],
        projects: [{ projectId: "project_1" as ProjectId, label: "OpenAIDE" }],
      },
      dispatch,
      scope: { kind: "taskNavigation" },
    });
    await Promise.resolve();

    eventListener?.({
      previousCursor: "cursor_1" as EventCursor,
      cursor: "cursor_2" as EventCursor,
      scope: { kind: "stateRoot", stateRootId: "root_1" as StateRootId },
      payload: {
        kind: "taskUpdated",
        task: taskSummary("task_1", "Finished task", "completed"),
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
      events(listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "events">;

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
      events(listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "events">;

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
      previousCursor: "cursor_1" as EventCursor,
      cursor: "cursor_2" as EventCursor,
      scope: { kind: "stateRoot", stateRootId: "root_1" as StateRootId },
      payload: { kind: "taskUpdated", task: taskSummary("task_1", "Older task", "running") },
    });
    eventListener?.({
      previousCursor: "cursor_2" as EventCursor,
      cursor: "cursor_3" as EventCursor,
      scope: { kind: "stateRoot", stateRootId: "root_1" as StateRootId },
      payload: { kind: "taskUpdated", task: taskSummary("task_1", "Included task", "completed") },
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
      events(listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "events">;

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
      events(listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "events">;

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
        events(listener: (event: AppServerEvent) => void) {
          eventListener = listener;
          return vi.fn();
        },
      } as Pick<BackendConnection, "request" | "events">;

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

  it("keeps the connection cursor across events owned by another subscription", async () => {
    let eventListener: ((event: AppServerEvent) => void) | undefined;
    const dispatch = vi.fn();
    const request = vi.fn(async () => taskSubscription("cursor_1", "task_1"));
    const connection = {
      request,
      events(listener: (event: AppServerEvent) => void) {
        eventListener = listener;
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "events">;

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
      previousCursor: "cursor_1" as EventCursor,
      cursor: "cursor_2" as EventCursor,
      scope: { kind: "stateRoot", stateRootId: "root_1" as StateRootId },
      payload: { kind: "taskUpdated", task: taskSummary("task_2", "Other task") },
    });
    eventListener?.(taskEvent("cursor_2", "cursor_3", "task_1"));
    await Promise.resolve();

    expect(request).toHaveBeenCalledTimes(1);
    expect(dispatch).toHaveBeenCalledTimes(2);
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
      events() {
        return stopEvents;
      },
    } as Pick<BackendConnection, "request" | "events">;

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
      events() {
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "events">;

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
      events() {
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "events">;
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
      events() {
        return vi.fn();
      },
    } as Pick<BackendConnection, "request" | "events">;
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
    previousCursor: previousCursor as EventCursor,
    cursor: cursor as EventCursor,
    scope: { kind: "stateRoot", stateRootId: "root_1" as StateRootId },
    payload: { kind: "taskNavigationUpdated", navigation: { tasks: [] } },
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
): AppServerEvent {
  return {
    previousCursor: previousCursor as EventCursor,
    cursor: cursor as EventCursor,
    scope: {
      kind: "task",
      stateRootId: stateRootId as StateRootId,
      taskId: taskIdValue as TaskId,
    },
    payload: {
      kind: "taskUpdated",
      task: taskSummary(taskIdValue, "Task"),
    },
  };
}

function textChunkEvent(
  previousCursor: string,
  cursor: string,
  text: string,
): AppServerEvent {
  return {
    previousCursor: previousCursor as EventCursor,
    cursor: cursor as EventCursor,
    scope: {
      kind: "task",
      stateRootId: "root_1" as StateRootId,
      taskId: "task_1" as TaskId,
    },
    payload: {
      kind: "chatItemChunk",
      taskId: "task_1" as TaskId,
      revision: Number(cursor.slice(-1)),
      messageId: "message_1" as MessageId,
      chunk: { text },
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
