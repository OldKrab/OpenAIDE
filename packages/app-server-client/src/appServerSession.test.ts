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
import {
  CLIENT_HEARTBEAT,
  STATE_SUBSCRIBE,
  STATE_UNSUBSCRIBE,
  TASK_LIST,
} from "./generated/protocol";
import { createAppServerSession } from "./appServerSession";

describe("AppServerSession", () => {
  it("authoritatively refreshes an idle scope when the browser returns to the foreground", async () => {
    let subscribeCount = 0;
    const raw = fakeConnection(async (method) => {
      if (method === CLIENT_HEARTBEAT) return {};
      if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
      subscribeCount += 1;
      return taskSubscription(`cursor_${subscribeCount}`, subscribeCount);
    });
    const session = createAppServerSession(raw.connection);
    await session.initialize(initializeParams());
    const snapshots: number[] = [];
    const baselineLost = vi.fn();
    session.subscribeState({ kind: "task", taskId: "task_1" as TaskId }, {
      onBaselineLost: baselineLost,
      onSnapshot(snapshot) {
        if (snapshot.kind === "task") snapshots.push(snapshot.task.revision);
      },
    });
    await vi.waitFor(() => expect(snapshots).toEqual([1]));

    for (let index = 0; index < 20; index += 1) raw.wake();

    await vi.waitFor(() => expect(snapshots).toEqual([1, 2]));
    expect(baselineLost).toHaveBeenCalledOnce();
    expect(subscribeCount).toBe(2);
    session.close();
  });

  it("exhausts one bounded wake recovery window and recovers on a later wake without a retry storm", async () => {
    vi.useFakeTimers();
    try {
      let recovering = false;
      let subscribeCount = 0;
      let taskListCount = 0;
      const raw = fakeConnection(async (method) => {
        if (method === CLIENT_HEARTBEAT) return {};
        if (method === TASK_LIST) {
          taskListCount += 1;
          return { tasks: [], revision: 1 };
        }
        if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
        subscribeCount += 1;
        if (subscribeCount === 1 || !recovering) {
          return taskSubscription(`cursor_${subscribeCount}`, subscribeCount);
        }
        throw new Error("baseline unavailable");
      });
      const session = createAppServerSession(raw.connection);
      const statuses: string[] = [];
      session.handleSessionStatus(({ status }) => statuses.push(status));
      await session.initialize(initializeParams());
      const snapshots: number[] = [];
      session.subscribeState({ kind: "task", taskId: "task_1" as TaskId }, {
        onSnapshot(snapshot) {
          if (snapshot.kind === "task") snapshots.push(snapshot.task.revision);
        },
      });
      await vi.advanceTimersByTimeAsync(0);
      expect(snapshots).toEqual([1]);

      recovering = true;
      raw.wake();
      await vi.advanceTimersByTimeAsync(30_000);

      expect(statuses.at(-1)).toBe("unavailable");
      expect(subscribeCount).toBe(6);
      for (let index = 0; index < 20; index += 1) {
        raw.emit(taskEvent("cursor_1", `queued_${index}`, index + 2));
      }
      await vi.advanceTimersByTimeAsync(60_000);
      expect(subscribeCount).toBe(6);
      await expect(session.request(TASK_LIST, { archived: false })).rejects.toThrow(
        "baseline unavailable",
      );
      expect(taskListCount).toBe(0);

      recovering = false;
      raw.wake();
      await vi.advanceTimersByTimeAsync(0);

      expect(statuses.at(-1)).toBe("ready");
      expect(snapshots).toEqual([1, 7]);
      expect(subscribeCount).toBe(7);
      await expect(session.request(TASK_LIST, { archived: false })).resolves.toEqual({
        tasks: [],
        revision: 1,
      });
      expect(taskListCount).toBe(1);
      session.close();
    } finally {
      vi.useRealTimers();
    }
  });

  it("proves idle liveness once and gates requests when wake has no active stream", async () => {
    const heartbeat = deferred<Record<symbol, never>>();
    let heartbeatCount = 0;
    let taskListCount = 0;
    const raw = fakeConnection(async (method) => {
      if (method === CLIENT_HEARTBEAT) {
        heartbeatCount += 1;
        return heartbeat.promise;
      }
      if (method === TASK_LIST) {
        taskListCount += 1;
        return { tasks: [], revision: 1 };
      }
      throw new Error(`Unexpected request: ${method}`);
    });
    const session = createAppServerSession(raw.connection);
    const statuses: string[] = [];
    session.handleSessionStatus(({ status }) => statuses.push(status));
    await session.initialize(initializeParams());

    for (let index = 0; index < 20; index += 1) raw.wake();
    const taskList = session.request(TASK_LIST, { archived: false });
    await Promise.resolve();

    expect(heartbeatCount).toBe(1);
    expect(taskListCount).toBe(0);
    expect(statuses.at(-1)).toBe("recovering");

    heartbeat.resolve({});
    await expect(taskList).resolves.toEqual({ tasks: [], revision: 1 });
    expect(taskListCount).toBe(1);
    expect(statuses.at(-1)).toBe("ready");
    session.close();
  });

  it("coalesces a foreground refresh with an overlapping physical-generation baseline", async () => {
    const recovery = new Map([
      ["task_1", deferred<StateSubscribeResult>()],
      ["task_2", deferred<StateSubscribeResult>()],
    ]);
    const repeated = new Map([
      ["task_1", deferred<StateSubscribeResult>()],
      ["task_2", deferred<StateSubscribeResult>()],
    ]);
    const subscribeCounts = new Map<string, number>();
    const raw = fakeConnection(async (method, params) => {
      if (method === CLIENT_HEARTBEAT) return {};
      if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
      const scope = (params as { scope: { kind: "task"; taskId: TaskId } }).scope;
      const taskId = scope.taskId;
      const count = (subscribeCounts.get(taskId) ?? 0) + 1;
      subscribeCounts.set(taskId, count);
      if (count === 1) return taskSubscription(`cursor_${taskId}_1`, 1, taskId);
      const pending = (count === 2 ? recovery : repeated).get(taskId);
      if (!pending) throw new Error(`Unexpected Task subscription: ${taskId}`);
      return pending.promise;
    });
    const session = createAppServerSession(raw.connection);
    const statuses: string[] = [];
    session.handleSessionStatus(({ status }) => statuses.push(status));
    await session.initialize(initializeParams());
    const firstReady = vi.fn();
    const secondReady = vi.fn();
    session.subscribeState(
      { kind: "task", taskId: "task_1" as TaskId },
      { onBaselineReady: firstReady, onSnapshot: vi.fn() },
    );
    session.subscribeState(
      { kind: "task", taskId: "task_2" as TaskId },
      { onBaselineReady: secondReady, onSnapshot: vi.fn() },
    );
    await vi.waitFor(() => expect([...subscribeCounts.values()]).toEqual([1, 1]));

    raw.invalidate({ reason: "clientLivenessExpired" });
    raw.recover({ reason: "clientLivenessExpired", result: initializeResult() });
    await vi.waitFor(() => expect([...subscribeCounts.values()]).toEqual([2, 2]));
    recovery.get("task_1")?.resolve(taskSubscription("cursor_task_1_2", 2, "task_1" as TaskId));
    await vi.waitFor(() => expect(firstReady).toHaveBeenCalledTimes(2));

    raw.wake();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect([...subscribeCounts.values()]).toEqual([2, 2]);
    recovery.get("task_2")?.resolve(taskSubscription("cursor_task_2_2", 2, "task_2" as TaskId));
    await vi.waitFor(() => expect(statuses.at(-1)).toBe("ready"));
    expect(firstReady).toHaveBeenCalledTimes(2);
    expect(secondReady).toHaveBeenCalledTimes(2);
    expect([...subscribeCounts.values()]).toEqual([2, 2]);
    session.close();
  });

  it("joins physical recovery when the wake heartbeat is closed by generation replacement", async () => {
    const heartbeat = deferred<Record<symbol, never>>();
    const replacement = deferred<StateSubscribeResult>();
    let subscribeCount = 0;
    const raw = fakeConnection(async (method) => {
      if (method === CLIENT_HEARTBEAT) return heartbeat.promise;
      if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
      subscribeCount += 1;
      return subscribeCount === 1
        ? taskSubscription("cursor_1", 1)
        : replacement.promise;
    });
    const session = createAppServerSession(raw.connection);
    const statuses: string[] = [];
    session.handleSessionStatus(({ status }) => statuses.push(status));
    await session.initialize(initializeParams());
    session.subscribeState(
      { kind: "task", taskId: "task_1" as TaskId },
      { onSnapshot: vi.fn() },
    );
    await vi.waitFor(() => expect(subscribeCount).toBe(1));

    raw.wake();
    raw.invalidate({ reason: "clientLivenessExpired" });
    heartbeat.reject(new Error("RPC peer is closed"));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(statuses).not.toContain("unavailable");
    raw.recover({ reason: "clientLivenessExpired", result: initializeResult() });
    await vi.waitFor(() => expect(subscribeCount).toBe(2));
    replacement.resolve(taskSubscription("cursor_2", 2));
    await vi.waitFor(() => expect(statuses.at(-1)).toBe("ready"));
    expect(statuses).not.toContain("unavailable");
    session.close();
  });

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

    await vi.waitFor(() => expect(snapshots).toEqual([1, 4]));
    expect(subscribeCount).toBe(2);
    session.close();
  });

  it("serializes and reruns refresh when pending events still have a cursor gap", async () => {
    const second = deferred<StateSubscribeResult>();
    const third = deferred<StateSubscribeResult>();
    let subscribeCount = 0;
    const raw = fakeConnection(async (method) => {
      if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
      subscribeCount += 1;
      if (subscribeCount === 1) return taskSubscription("cursor_1", 1);
      if (subscribeCount === 2) return second.promise;
      return third.promise;
    });
    const session = createAppServerSession(raw.connection);
    await session.initialize(initializeParams());
    const snapshots: number[] = [];
    const ready = vi.fn();
    session.subscribeState({ kind: "task", taskId: "task_1" as TaskId }, {
      onBaselineReady: ready,
      onSnapshot(snapshot) {
        if (snapshot.kind === "task") snapshots.push(snapshot.task.revision);
      },
    });
    await vi.waitFor(() => expect(snapshots).toEqual([1]));

    raw.emit(taskEvent("missing_cursor", "cursor_3", 3));
    await vi.waitFor(() => expect(subscribeCount).toBe(2));
    raw.emit(taskEvent("cursor_3", "cursor_4", 4));
    second.resolve(taskSubscription("cursor_2", 2));

    await vi.waitFor(() => expect(subscribeCount).toBe(3));
    expect(ready).toHaveBeenCalledTimes(1);
    third.resolve(taskSubscription("cursor_4", 4));

    await vi.waitFor(() => expect(ready).toHaveBeenCalledTimes(2));
    expect(snapshots).toEqual([1, 4]);
    session.close();
  });

  it("retains a live gap across stale baselines until the replica reconciles", async () => {
    vi.useFakeTimers();
    try {
      const refreshes = [
        deferred<StateSubscribeResult>(),
        deferred<StateSubscribeResult>(),
        deferred<StateSubscribeResult>(),
      ];
      let subscribeCount = 0;
      const raw = fakeConnection(async (method) => {
        if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
        subscribeCount += 1;
        if (subscribeCount === 1) return taskSubscription("cursor_1", 1);
        const refresh = refreshes[subscribeCount - 2];
        if (!refresh) throw new Error("Unexpected subscription retry");
        return refresh.promise;
      });
      const session = createAppServerSession(raw.connection);
      await session.initialize(initializeParams());
      const snapshots: number[] = [];
      session.subscribeState({ kind: "task", taskId: "task_1" as TaskId }, {
        onSnapshot(snapshot) {
          if (snapshot.kind === "task") snapshots.push(snapshot.task.revision);
        },
      });
      await vi.advanceTimersByTimeAsync(0);
      expect(snapshots).toEqual([1]);

      raw.emit(taskEvent("cursor_2", "cursor_3", 3));
      raw.emit(taskEvent("cursor_3", "cursor_4", 4));
      await vi.advanceTimersByTimeAsync(0);
      expect(subscribeCount).toBe(2);

      refreshes[0]?.resolve(taskSubscription("cursor_1", 2));
      await vi.advanceTimersByTimeAsync(500);
      expect(subscribeCount).toBe(3);
      expect(snapshots).toEqual([1]);

      refreshes[1]?.resolve(taskSubscription("cursor_1", 2));
      await vi.advanceTimersByTimeAsync(999);
      expect(subscribeCount).toBe(3);
      expect(snapshots).toEqual([1]);
      await vi.advanceTimersByTimeAsync(1);
      expect(subscribeCount).toBe(4);

      refreshes[2]?.resolve(taskSubscription("cursor_2", 2));
      await vi.advanceTimersByTimeAsync(0);
      expect(snapshots).toEqual([1, 4]);
      expect(subscribeCount).toBe(4);
      await vi.advanceTimersByTimeAsync(10_000);
      expect(subscribeCount).toBe(4);
      session.close();
    } finally {
      vi.useRealTimers();
    }
  });

  it("backs repeated cursor-gap refreshes off and later publishes only the reconciled baseline", async () => {
    vi.useFakeTimers();
    try {
      const refreshes = [deferred<StateSubscribeResult>(), deferred<StateSubscribeResult>()];
      let subscribeCount = 0;
      const raw = fakeConnection(async (method) => {
        if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
        subscribeCount += 1;
        if (subscribeCount === 1) return taskSubscription("cursor_1", 1);
        const refresh = refreshes[subscribeCount - 2];
        if (!refresh) return taskSubscription("cursor_6", 6);
        return refresh.promise;
      });
      const session = createAppServerSession(raw.connection);
      await session.initialize(initializeParams());
      const snapshots: number[] = [];
      session.subscribeState({ kind: "task", taskId: "task_1" as TaskId }, {
        onSnapshot(snapshot) {
          if (snapshot.kind === "task") snapshots.push(snapshot.task.revision);
        },
      });
      await vi.advanceTimersByTimeAsync(0);
      expect(snapshots).toEqual([1]);

      raw.emit(taskEvent("missing_cursor", "cursor_3", 3));
      await vi.advanceTimersByTimeAsync(0);
      raw.emit(taskEvent("cursor_3", "cursor_4", 4));
      refreshes[0]?.resolve(taskSubscription("cursor_2", 2));
      await vi.advanceTimersByTimeAsync(0);
      expect(subscribeCount).toBe(2);
      expect(snapshots).toEqual([1]);

      await vi.advanceTimersByTimeAsync(499);
      expect(subscribeCount).toBe(2);
      await vi.advanceTimersByTimeAsync(1);
      expect(subscribeCount).toBe(3);
      raw.emit(taskEvent("cursor_5", "cursor_6", 6));
      refreshes[1]?.resolve(taskSubscription("cursor_4", 4));
      await vi.advanceTimersByTimeAsync(0);
      expect(subscribeCount).toBe(3);
      expect(snapshots).toEqual([1]);

      await vi.advanceTimersByTimeAsync(999);
      expect(subscribeCount).toBe(3);
      await vi.advanceTimersByTimeAsync(1);
      expect(subscribeCount).toBe(4);
      await vi.advanceTimersByTimeAsync(0);
      expect(snapshots).toEqual([1, 6]);
      session.close();
    } finally {
      vi.useRealTimers();
    }
  });

  it("cancels a cursor-gap retry when its last observer closes", async () => {
    vi.useFakeTimers();
    try {
      const replacement = deferred<StateSubscribeResult>();
      let subscribeCount = 0;
      const raw = fakeConnection(async (method) => {
        if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
        subscribeCount += 1;
        return subscribeCount === 1 ? taskSubscription("cursor_1", 1) : replacement.promise;
      });
      const session = createAppServerSession(raw.connection);
      await session.initialize(initializeParams());
      const stop = session.subscribeState(
        { kind: "task", taskId: "task_1" as TaskId },
        { onSnapshot: vi.fn() },
      );
      await vi.advanceTimersByTimeAsync(0);
      raw.emit(taskEvent("missing_cursor", "cursor_3", 3));
      await vi.advanceTimersByTimeAsync(0);
      raw.emit(taskEvent("cursor_3", "cursor_4", 4));
      replacement.resolve(taskSubscription("cursor_2", 2));
      await vi.advanceTimersByTimeAsync(0);
      expect(subscribeCount).toBe(2);

      stop();
      await vi.advanceTimersByTimeAsync(30_000);
      expect(subscribeCount).toBe(2);
      session.close();
    } finally {
      vi.useRealTimers();
    }
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

  it("backs failed subscription refreshes off to a bounded request rate", async () => {
    vi.useFakeTimers();
    try {
      let requestCount = 0;
      const raw = fakeConnection(async (method) => {
        if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
        requestCount += 1;
        throw new Error("temporarily unavailable");
      });
      const session = createAppServerSession(raw.connection);
      await session.initialize(initializeParams());
      const errors = vi.fn();
      session.subscribeState(
        { kind: "task", taskId: "task_1" as TaskId },
        { onBaselineError: errors, onSnapshot: vi.fn() },
      );

      await vi.advanceTimersByTimeAsync(0);
      expect(requestCount).toBe(1);
      await vi.advanceTimersByTimeAsync(499);
      expect(requestCount).toBe(1);
      await vi.advanceTimersByTimeAsync(1_001);
      expect(requestCount).toBe(3);
      await vi.advanceTimersByTimeAsync(2_000);
      expect(requestCount).toBe(4);
      await vi.advanceTimersByTimeAsync(4_000);
      expect(requestCount).toBe(5);
      await vi.advanceTimersByTimeAsync(10_000);
      expect(requestCount).toBe(5);
      expect(errors).toHaveBeenCalledTimes(5);
      session.close();
    } finally {
      vi.useRealTimers();
    }
  });
});

function fakeConnection(
  requestImplementation: (method: string, params: unknown) => Promise<unknown>,
) {
  const eventListeners = new Set<(event: AppServerEvent) => void>();
  const invalidationListeners = new Set<(event: BackendGenerationInvalidation) => void>();
  const baselineListeners = new Set<(event: BackendRecoveryBaseline) => void>();
  const failureListeners = new Set<(event: BackendRecoveryFailure) => void>();
  const wakeListeners = new Set<() => void>();
  const connection = {
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
    handleWake(handler: () => void) {
      wakeListeners.add(handler);
      return () => wakeListeners.delete(handler);
    },
    close() {},
  } as BackendConnection;
  return {
    connection,
    emit(event: AppServerEvent) {
      for (const listener of eventListeners) listener(event);
    },
    wake() {
      for (const listener of wakeListeners) listener();
    },
    invalidate(event: BackendGenerationInvalidation) {
      for (const listener of invalidationListeners) listener(event);
    },
    recover(event: BackendRecoveryBaseline) {
      for (const listener of baselineListeners) listener(event);
    },
  };
}

function taskSubscription(
  cursor: string,
  revision: number,
  taskId = "task_1" as TaskId,
): StateSubscribeResult {
  return {
    cursor: cursor as EventCursor,
    scope: { kind: "task", taskId },
    snapshot: {
      kind: "task",
      task: {
        task: taskSummary(taskId),
        lifecycle: "visible",
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

function taskSummary(taskId = "task_1" as TaskId) {
  return {
    taskId,
    projectId: "project_1" as never,
    agentId: "codex" as never,
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
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, reject, resolve };
}
