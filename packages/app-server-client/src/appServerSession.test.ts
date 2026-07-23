import { describe, expect, it, vi } from "vitest";
import type {
  AppServerEvent,
  BackendConnection,
  BackendGenerationInvalidation,
  BackendRecoveryBaseline,
  BackendRecoveryFailure,
  EventCursor,
  InitializeResult,
  StateSubscribeResult,
  TaskId,
  WorktreeRepositoryId,
} from "./index";
import { STATE_SUBSCRIBE, STATE_UNSUBSCRIBE } from "./generated/protocol";
import { createAppServerSession } from "./appServerSession";

describe("AppServerSession", () => {
  it("refreshes once for a cursor gap and replays events received behind the new baseline", async () => {
    const replacement = deferred<StateSubscribeResult>();
    let subscribeCount = 0;
    const raw = fakeConnection(async (method) => {
      if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
      subscribeCount += 1;
      return subscribeCount === 1
        ? taskSubscription("cursor_1", 1)
        : replacement.promise;
    });
    const session = createAppServerSession(raw.connection);
    await session.initialize(initializeParams());
    const snapshots: number[] = [];
    session.subscribeState({ kind: "task", taskId: "task_1" as TaskId }, {
      onSnapshot(snapshot) {
        if (snapshot.kind === "task") snapshots.push(snapshot.task.revision);
      },
    });
    await vi.waitFor(() => expect(snapshots).toEqual([1]));

    raw.emit(taskEvent("missing_cursor", "cursor_2", 2));
    raw.emit(taskEvent("cursor_2", "cursor_3", 3));
    raw.emit(taskEvent("cursor_3", "cursor_4", 4));
    await vi.waitFor(() => expect(subscribeCount).toBe(2));
    replacement.resolve(taskSubscription("cursor_2", 2));

    await vi.waitFor(() => expect(snapshots).toEqual([1, 2, 3, 4]));
    expect(subscribeCount).toBe(2);
    session.close();
  });

  it("shares one scope replica and unsubscribes only after its last observer leaves", async () => {
    const requests: string[] = [];
    const raw = fakeConnection(async (method) => {
      requests.push(method);
      if (method === STATE_SUBSCRIBE) return taskSubscription("cursor_1", 1);
      if (method === STATE_UNSUBSCRIBE) {
        return { scope: { kind: "task", taskId: "task_1" as TaskId } };
      }
      throw new Error(`Unexpected request: ${method}`);
    });
    const session = createAppServerSession(raw.connection);
    await session.initialize(initializeParams());
    const first = vi.fn();
    const second = vi.fn();
    const stopFirst = session.subscribeState(
      { kind: "task", taskId: "task_1" as TaskId },
      { onSnapshot: first },
    );
    const stopSecond = session.subscribeState(
      { kind: "task", taskId: "task_1" as TaskId },
      { onSnapshot: second },
    );
    await vi.waitFor(() => expect(first).toHaveBeenCalledOnce());
    await vi.waitFor(() => expect(second).toHaveBeenCalledOnce());
    expect(requests).toEqual([STATE_SUBSCRIBE]);

    stopFirst();
    expect(requests).toEqual([STATE_SUBSCRIBE]);
    stopSecond();

    await vi.waitFor(() => expect(requests).toEqual([STATE_SUBSCRIBE, STATE_UNSUBSCRIBE]));
    session.close();
  });

  it("maintains an independent replica for each Worktree Repository", async () => {
    const subscribedRepositories: string[] = [];
    const raw = fakeConnection(async (method, params) => {
      if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
      const scope = (params as { scope: { kind: "worktreeRepository"; repositoryId: WorktreeRepositoryId } }).scope;
      subscribedRepositories.push(scope.repositoryId);
      return {
        cursor: `cursor_${subscribedRepositories.length}` as EventCursor,
        scope,
        snapshot: {
          kind: "worktreeRepository",
          repository: { repositoryId: scope.repositoryId, revision: 1, worktrees: [] },
        },
      } as unknown as StateSubscribeResult;
    });
    const session = createAppServerSession(raw.connection);
    await session.initialize(initializeParams());

    session.subscribeState(
      { kind: "worktreeRepository", repositoryId: "repository_1" as WorktreeRepositoryId },
      { onSnapshot: vi.fn() },
    );
    session.subscribeState(
      { kind: "worktreeRepository", repositoryId: "repository_2" as WorktreeRepositoryId },
      { onSnapshot: vi.fn() },
    );

    await vi.waitFor(() => expect(subscribedRepositories).toEqual(["repository_1", "repository_2"]));
    session.close();
  });
});

function fakeConnection(
  requestImplementation: (method: string, params: unknown) => Promise<unknown>,
) {
  const eventListeners = new Set<(event: AppServerEvent) => void>();
  const invalidationListeners = new Set<(event: BackendGenerationInvalidation) => void>();
  const baselineListeners = new Set<(event: BackendRecoveryBaseline) => void>();
  const failureListeners = new Set<(event: BackendRecoveryFailure) => void>();
  const connection: BackendConnection = {
    async initialize() {
      return initializeResult();
    },
    request: vi.fn(requestImplementation) as unknown as BackendConnection["request"],
    handleRequest() {
      return () => undefined;
    },
    handleNotification(_method, handler) {
      eventListeners.add(handler);
      return () => eventListeners.delete(handler);
    },
    handleGenerationInvalidated(handler) {
      invalidationListeners.add(handler);
      return () => invalidationListeners.delete(handler);
    },
    handleRecoveryBaseline(handler) {
      baselineListeners.add(handler);
      return () => baselineListeners.delete(handler);
    },
    handleRecoveryFailed(handler) {
      failureListeners.add(handler);
      return () => failureListeners.delete(handler);
    },
    close() {},
  };
  return {
    connection,
    emit(event: AppServerEvent) {
      for (const listener of eventListeners) listener(event);
    },
  };
}

function taskSubscription(cursor: string, revision: number): StateSubscribeResult {
  return {
    cursor: cursor as EventCursor,
    scope: { kind: "task", taskId: "task_1" as TaskId },
    snapshot: {
      kind: "task",
      task: {
        task: taskSummary(),
        lifecycle: "open",
        revision,
        preparation: { kind: "ready" },
        agentConfig: { state: "ready", options: [] },
        agentCommands: { state: "ready", commands: [] },
        sendCapability: { state: "ready" },
        historySync: { state: "idle", generation: 0 },
        chat: { items: [], hasMessages: false },
        pendingRequests: [],
      },
    },
  };
}

function taskEvent(previousCursor: string, cursor: string, revision: number): AppServerEvent {
  return {
    subscription: { kind: "task", taskId: "task_1" as TaskId },
    previousCursor: previousCursor as EventCursor,
    cursor: cursor as EventCursor,
    scope: { kind: "task", stateRootId: "root_1" as never, taskId: "task_1" as TaskId },
    payload: {
      kind: "taskChanged",
      taskId: "task_1" as TaskId,
      revision,
      changes: { task: taskSummary() },
    },
  };
}

function taskSummary() {
  return {
    taskId: "task_1" as TaskId,
    projectId: "project_1" as never,
    agentId: "codex" as never,
    lifecycle: "open" as const,
    title: { value: "Recovered Task", source: "user" as const },
    status: "idle" as const,
    updatedAt: "2026-07-18T00:00:00.000Z",
    lastActivity: "2026-07-18T00:00:00.000Z",
    unread: false,
    hasMessages: true,
    workspaceAvailable: true,
  };
}

function initializeParams() {
  return {
    clientInstanceId: "client_1" as never,
    shell: { kind: "web" as const },
    requestedSurface: { kind: "home" as const },
    capabilities: { protocol: [], shell: [] },
  };
}

function initializeResult(): InitializeResult {
  return {
    snapshot: {
      cursor: "cursor_initial" as EventCursor,
      server: { serverId: "server_1" as never, protocolVersion: { major: 1, minor: 0 }, capabilities: {} },
      stateRoot: { stateRootId: "root_1" as never },
      client: { clientInstanceId: "client_1" as never, shellKind: "web", surface: { kind: "home" } },
      newTaskDefaults: { projectId: null, agentId: null },
      pendingRequests: [],
    },
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
