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
import { STATE_SUBSCRIBE, STATE_UNSUBSCRIBE, TASK_CANCEL } from "./generated/protocol";
import { createAppServerSession } from "./appServerSession";

describe("AppServerSession", () => {
  it("keeps initialization behind a recovery that supersedes its first scope baseline", async () => {
    vi.useFakeTimers();
    const obsolete = deferred<StateSubscribeResult>();
    const replacement = deferred<StateSubscribeResult>();
    let subscriptions = 0;
    const raw = fakeConnection(async (method) => {
      if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
      return ++subscriptions === 1 ? obsolete.promise : replacement.promise;
    });
    const session = createAppServerSession(raw.connection);
    const initialized = vi.fn();
    const recovered = initializeResult();
    recovered.snapshot.cursor = "cursor_recovered" as EventCursor;
    const statuses: string[] = [];
    session.handleSessionStatus((status) => statuses.push(status.status));
    session.subscribeState({ kind: "task", taskId: "task_1" as TaskId }, { onSnapshot: vi.fn() });
    const initialization = session.initialize(initializeParams()).then(initialized);
    try {
      await vi.advanceTimersByTimeAsync(0);
      expect(subscriptions).toBe(1);
      raw.invalidate();
      raw.recover(recovered);
      await vi.advanceTimersByTimeAsync(0);
      expect(subscriptions).toBe(2);

      obsolete.resolve(taskSubscription("obsolete", 1));
      await vi.advanceTimersByTimeAsync(0);
      expect(statuses.at(-1)).toBe("recovering");
      expect(initialized).not.toHaveBeenCalled();

      replacement.resolve(taskSubscription("replacement", 2));
      await initialization;
      expect(statuses.at(-1)).toBe("ready");
      expect(initialized).toHaveBeenCalledOnce();
      expect(initialized).toHaveBeenCalledWith(recovered);
    } finally {
      session.close();
      vi.useRealTimers();
    }
  });

  it.each(["failed", "closed"])("does not declare initialization ready after recovery is %s", async (outcome) => {
    vi.useFakeTimers();
    const obsolete = deferred<StateSubscribeResult>();
    const raw = fakeConnection(async () => obsolete.promise);
    const session = createAppServerSession(raw.connection);
    const statuses: string[] = [];
    session.handleSessionStatus((status) => statuses.push(status.status));
    session.subscribeState({ kind: "task", taskId: "task_1" as TaskId }, { onSnapshot: vi.fn() });
    const initialization = session.initialize(initializeParams()).then(() => "ready", (error: Error) => error.message);
    try {
      await vi.advanceTimersByTimeAsync(0);
      raw.invalidate();
      if (outcome === "failed") raw.fail(new Error("Recovery failed"));
      else session.close();
      obsolete.resolve(taskSubscription("obsolete", 1));

      await expect(initialization).resolves.toMatch(outcome === "failed" ? /Recovery failed/ : /closed/);
      expect(statuses).not.toContain("ready");
    } finally {
      session.close();
      vi.useRealTimers();
    }
  });

  it("replaces a baseline when buffered events reveal a gap before declaring the replica ready", async () => {
    const initial = deferred<StateSubscribeResult>();
    const replacement = deferred<StateSubscribeResult>();
    let subscribeCount = 0;
    const raw = fakeConnection(async (method) => {
      if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
      subscribeCount += 1;
      return subscribeCount === 1 ? initial.promise : replacement.promise;
    });
    const session = createAppServerSession(raw.connection);
    await session.initialize(initializeParams());
    const ready = vi.fn();
    const snapshots: number[] = [];
    session.subscribeState({ kind: "task", taskId: "task_1" as TaskId }, {
      onBaselineReady: ready,
      onSnapshot(snapshot) {
        if (snapshot.kind === "task") snapshots.push(snapshot.task.revision);
      },
    });

    // The baseline ends at revision 1, but only revision 3 reaches the client.
    raw.emit(taskEvent("cursor_2", "cursor_3", 3));
    initial.resolve(taskSubscription("cursor_1", 1));
    try {
      await vi.waitFor(() => expect(subscribeCount).toBe(2));
      expect(ready).not.toHaveBeenCalled();
      replacement.resolve(taskSubscription("cursor_3", 3));
      await vi.waitFor(() => expect(snapshots.at(-1)).toBe(3));
      expect(ready).toHaveBeenCalledOnce();
    } finally {
      session.close();
    }
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

    await vi.waitFor(() => expect(snapshots).toEqual([1, 2, 3, 4]));
    expect(subscribeCount).toBe(2);
    session.close();
  });

  it("keeps a gap-invalidated replica unavailable while a replacement baseline fails and retries", async () => {
    vi.useFakeTimers();
    const initial = deferred<StateSubscribeResult>();
    const replacement = deferred<StateSubscribeResult>();
    let subscribeCount = 0;
    const raw = fakeConnection(async (method) => {
      if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
      subscribeCount += 1;
      if (subscribeCount === 1) return initial.promise;
      if (subscribeCount === 2) throw new Error("Baseline temporarily unavailable");
      return replacement.promise;
    });
    const session = createAppServerSession(raw.connection);
    try {
      await session.initialize(initializeParams());
      const ready = vi.fn();
      const failed = vi.fn();
      const snapshots: number[] = [];
      session.subscribeState({ kind: "task", taskId: "task_1" as TaskId }, {
        onBaselineReady: ready,
        onBaselineError: failed,
        onSnapshot(snapshot) {
          if (snapshot.kind === "task") snapshots.push(snapshot.task.revision);
        },
      });
      raw.emit(taskEvent("cursor_2", "cursor_3", 3));
      initial.resolve(taskSubscription("cursor_1", 1));
      await vi.advanceTimersByTimeAsync(0);
      expect(failed).toHaveBeenCalledOnce();
      expect(ready).not.toHaveBeenCalled();

      await vi.advanceTimersByTimeAsync(500);
      expect(subscribeCount).toBe(3);
      expect(ready).not.toHaveBeenCalled();
      replacement.resolve(taskSubscription("cursor_3", 3));
      await vi.advanceTimersByTimeAsync(0);
      expect(snapshots.at(-1)).toBe(3);
      expect(ready).toHaveBeenCalledOnce();
    } finally {
      session.close();
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

  it("invalidates every scope again when another transport expires during baseline recovery", async () => {
    const obsoleteProjects = deferred<StateSubscribeResult>();
    const latestProjects = deferred<StateSubscribeResult>();
    let taskSubscriptions = 0;
    let projectSubscriptions = 0;
    const mutation = vi.fn();
    const projects = (cursor: string): StateSubscribeResult => ({
      scope: { kind: "projects" },
      cursor: cursor as EventCursor,
      snapshot: { kind: "projects", projects: { projects: [] } },
    });
    const raw = fakeConnection(async (method, params) => {
      if (method === TASK_CANCEL) {
        mutation();
        return {};
      }
      if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
      if ((params as { scope: { kind: string } }).scope.kind === "task") {
        taskSubscriptions += 1;
        return taskSubscription(`task_${taskSubscriptions}`, taskSubscriptions);
      }
      projectSubscriptions += 1;
      if (projectSubscriptions === 1) return projects("projects_1");
      return projectSubscriptions === 2 ? obsoleteProjects.promise : latestProjects.promise;
    });
    const session = createAppServerSession(raw.connection);
    try {
      await session.initialize(initializeParams());
      const snapshots: number[] = [];
      session.subscribeState({ kind: "task", taskId: "task_1" as TaskId }, {
        onSnapshot(snapshot) {
          if (snapshot.kind === "task") snapshots.push(snapshot.task.revision);
        },
      });
      session.subscribeState({ kind: "projects" }, { onSnapshot: vi.fn() });
      await vi.waitFor(() => expect(snapshots).toEqual([1]));

      raw.invalidate();
      raw.recover();
      await vi.waitFor(() => expect(snapshots).toEqual([1, 2]));
      const pendingMutation = session.request(TASK_CANCEL, { taskId: "task_1" as TaskId });
      raw.invalidate();
      raw.recover();
      await vi.waitFor(() => expect(snapshots).toEqual([1, 2, 3]));
      obsoleteProjects.resolve(projects("projects_2"));
      await Promise.resolve();
      expect(mutation).not.toHaveBeenCalled();

      latestProjects.resolve(projects("projects_3"));
      await pendingMutation;
      expect(mutation).toHaveBeenCalledOnce();
      expect(projectSubscriptions).toBe(3);
    } finally {
      session.close();
    }
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
    invalidate() {
      for (const listener of invalidationListeners) listener({ reason: "httpSessionExpired" });
    },
    recover(result = initializeResult()) {
      for (const listener of baselineListeners) listener({
        reason: "httpSessionExpired",
        result,
      });
    },
    fail(error: unknown) {
      for (const listener of failureListeners) listener({ reason: "httpSessionExpired", error });
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
        permissionPolicy: "askEveryTime",
        preparation: { kind: "ready" },
        agentConfig: { state: "ready", options: [] },
        agentCommands: { state: "ready", commands: [] },
        sendCapability: { state: "ready" },
        messageQueue: { revision: 0, items: [] },
        historySync: { state: "idle", generation: 0 },
        subagents: { totalCount: 0, runningCount: 0, attentionCount: 0, available: true },
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
