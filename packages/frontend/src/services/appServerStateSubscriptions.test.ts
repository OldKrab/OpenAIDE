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
  it("dispatches each ordered live text delta before finalization", async () => {
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
        item: {
          messageId: "message_1" as MessageId,
          role: "agent",
          status: "streaming",
          parts: [{ kind: "text", text: "One" }],
        },
      },
    });
    eventListener?.(textChunkEvent("cursor_2", "cursor_3", " two", false));
    eventListener?.(textChunkEvent("cursor_3", "cursor_4", "", true));

    const snapshots = dispatch.mock.calls
      .map(([action]) => action)
      .filter((action) => action.type === "snapshot");
    expect(snapshots).toHaveLength(4);
    expect(snapshots[1].snapshot.chat.items[0].message).toMatchObject({ text: "One", streaming: true });
    expect(snapshots[2].snapshot.chat.items[0].message).toMatchObject({ text: "One two", streaming: true });
    expect(snapshots[3].snapshot.chat.items[0].message).toMatchObject({ text: "One two", streaming: false });
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
            title: "Real task",
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
      tasks: [expect.objectContaining({
        task_id: "task_1",
        project_id: "project_1",
        project_label: "OpenAIDE",
        title: "Real task",
      })],
    });
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
            defaultAgentId: "opencode" as never,
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
      agentId: "opencode",
      agentLabel: "OpenCode",
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
      tasks: [expect.objectContaining({
        task_id: "task_1",
        title: "Finished task",
        status: "completed",
      })],
    });
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

function taskEvent(previousCursor: string, cursor: string, taskIdValue: string): AppServerEvent {
  return {
    previousCursor: previousCursor as EventCursor,
    cursor: cursor as EventCursor,
    scope: {
      kind: "task",
      stateRootId: "root_1" as StateRootId,
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
  finalChunk: boolean,
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
      messageId: "message_1" as MessageId,
      chunk: { sequence: Number(cursor.slice(-1)), text, finalChunk },
    },
  };
}

function protocolTaskSnapshot(taskIdValue: string) {
  return {
    task: taskSummary(taskIdValue, "Task"),
    revision: 1,
    preparation: { kind: "ready" as const },
    agentConfig: { state: "ready" as const, options: [] },
    agentCommands: { state: "ready" as const, commands: [] },
    sendCapability: { state: "ready" as const },
    chat: { items: [], hasMessages: false },
    pendingRequests: [],
  };
}

function taskSummary(taskIdValue: string, title: string, status: ProtocolTaskSummary["status"] = "idle"): ProtocolTaskSummary {
  return {
    taskId: taskIdValue as TaskId,
    projectId: "project_1" as ProjectId,
    agentId: "codex" as never,
    title,
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
