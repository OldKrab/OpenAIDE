import { describe, expect, it, vi } from "vitest";
import type {
  AppServerEvent,
  BackendConnection,
  BackendGenerationInvalidation,
  BackendRecoveryBaseline,
  BackendRecoveryFailure,
  ChatItem,
  EventCursor,
  InitializeResult,
  MessageId,
  StateSubscribeResult,
  SubagentChatSync,
  SubagentId,
  TaskChatSync,
  TaskId,
  ToolTerminalSync,
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

  it("declares a later initialization ready after an earlier one ended unavailable", async () => {
    vi.useFakeTimers();
    const obsolete = deferred<StateSubscribeResult>();
    const raw = fakeConnection(async () => obsolete.promise);
    const session = createAppServerSession(raw.connection);
    const statuses: string[] = [];
    session.handleSessionStatus((status) => statuses.push(status.status));
    session.subscribeState({ kind: "task", taskId: "task_1" as TaskId }, { onSnapshot: vi.fn() });
    const first = session.initialize(initializeParams()).then(() => "ready", (error: Error) => error.message);
    try {
      await vi.advanceTimersByTimeAsync(0);
      raw.invalidate();
      raw.fail(new Error("Recovery failed"));
      obsolete.resolve(taskSubscription("obsolete", 1));
      await expect(first).resolves.toMatch(/Recovery failed/);
      expect(statuses.at(-1)).toBe("unavailable");

      await expect(session.initialize(initializeParams())).resolves.toEqual(initializeResult());
      expect(statuses.at(-1)).toBe("ready");
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

    // Revision 2 follows the baseline, but the next event skips revision 3's link.
    raw.emit(taskEvent("cursor_1", "cursor_2", 2));
    raw.emit(taskEvent("cursor_3", "cursor_4", 4));
    initial.resolve(taskSubscription("cursor_1", 1));
    try {
      await vi.waitFor(() => expect(subscribeCount).toBe(2));
      expect(ready).not.toHaveBeenCalled();
      replacement.resolve(taskSubscription("cursor_4", 4));
      await vi.waitFor(() => expect(snapshots.at(-1)).toBe(4));
      expect(ready).toHaveBeenCalledOnce();
    } finally {
      session.close();
    }
  });

  it("drops buffered events the baseline already contains and applies those that follow it", async () => {
    const initial = deferred<StateSubscribeResult>();
    const raw = fakeConnection(async (method) => {
      if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
      return initial.promise;
    });
    const logger = recordingLogger();
    const session = createAppServerSession(raw.connection, logger);
    await session.initialize(initializeParams());
    const snapshots: number[] = [];
    session.subscribeState({ kind: "task", taskId: "task_1" as TaskId }, {
      onSnapshot(snapshot) {
        if (snapshot.kind === "task") snapshots.push(snapshot.task.revision);
      },
    });

    // App Server sends the subscription's earlier events before its reply, and later ones after.
    raw.emit(taskEvent("cursor_1", "cursor_2", 2));
    raw.emit(taskEvent("cursor_2", "cursor_3", 3));
    // Cursor values are unique, not consecutive: other subscriptions' events take values too.
    raw.emit(taskEvent("cursor_3", "cursor_5", 4));
    initial.resolve(taskSubscription("cursor_3", 3));
    try {
      await vi.waitFor(() => expect(snapshots).toEqual([3, 4]));
      expect(logger.error).not.toHaveBeenCalled();
      expect(logger.info).toHaveBeenCalledWith(
        "app_server_state_baseline_ready",
        expect.objectContaining({ dropped_pending_event_count: 2 }),
      );
    } finally {
      session.close();
    }
  });

  it("logs a cursor gap as an error and resyncs", async () => {
    const replacement = deferred<StateSubscribeResult>();
    let subscribeCount = 0;
    const raw = fakeConnection(async (method) => {
      if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
      subscribeCount += 1;
      return subscribeCount === 1 ? taskSubscription("cursor_1", 1) : replacement.promise;
    });
    const logger = recordingLogger();
    const session = createAppServerSession(raw.connection, logger);
    await session.initialize(initializeParams());
    const snapshots: number[] = [];
    session.subscribeState({ kind: "task", taskId: "task_1" as TaskId }, {
      onSnapshot(snapshot) {
        if (snapshot.kind === "task") snapshots.push(snapshot.task.revision);
      },
    });
    await vi.waitFor(() => expect(snapshots).toEqual([1]));

    raw.emit(taskEvent("cursor_2", "cursor_3", 3));
    try {
      await vi.waitFor(() => expect(subscribeCount).toBe(2));
      expect(logger.error).toHaveBeenCalledWith(
        "app_server_state_cursor_gap",
        expect.objectContaining({
          expected_previous_cursor: "cursor_1",
          previous_cursor: "cursor_2",
          cursor: "cursor_3",
        }),
      );
      replacement.resolve(taskSubscription("cursor_3", 3));
      await vi.waitFor(() => expect(snapshots.at(-1)).toBe(3));
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
      raw.emit(taskEvent("cursor_1", "cursor_2", 2));
      raw.emit(taskEvent("cursor_3", "cursor_4", 4));
      initial.resolve(taskSubscription("cursor_1", 1));
      await vi.advanceTimersByTimeAsync(0);
      expect(failed).toHaveBeenCalledOnce();
      expect(ready).not.toHaveBeenCalled();

      await vi.advanceTimersByTimeAsync(500);
      expect(subscribeCount).toBe(3);
      expect(ready).not.toHaveBeenCalled();
      replacement.resolve(taskSubscription("cursor_4", 4));
      await vi.advanceTimersByTimeAsync(0);
      expect(snapshots.at(-1)).toBe(4);
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
      for (const listener of invalidationListeners) listener({ reason: "sessionExpired" });
    },
    recover(result = initializeResult()) {
      for (const listener of baselineListeners) listener({
        reason: "sessionExpired",
        result,
      });
    },
    fail(error: unknown) {
      for (const listener of failureListeners) listener({ reason: "sessionExpired", error });
    },
  };
}

function taskSubscription(
  cursor: string,
  revision: number,
  chatVersion = revision,
  chatSync: TaskChatSync = { kind: "full" },
  chatItems: ChatItem[] = [],
): StateSubscribeResult {
  return {
    cursor: cursor as EventCursor,
    scope: { kind: "task", taskId: "task_1" as TaskId },
    snapshot: {
      kind: "task",
      chatVersion,
      chatSync,
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
        chat: { items: chatItems, hasMessages: chatItems.length > 0 },
        pendingRequests: [],
      },
    },
  };
}

function userItem(messageId: string): ChatItem {
  const id = messageId as MessageId;
  return {
    kind: "user",
    messageId: id,
    cursor: messageId,
    parts: [{ kind: "text", text: messageId }],
  } as unknown as ChatItem;
}

function recordingLogger() {
  return { info: vi.fn(), warn: vi.fn(), error: vi.fn() };
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

describe("AppServerSession Task resume", () => {
  const scope = { kind: "task" as const, taskId: "task_1" as TaskId };

  function resume(
    revision: number,
    chatVersion: number,
    chatSync: TaskChatSync,
    cursor = "cursor_resume",
  ): StateSubscribeResult {
    const reply = taskSubscription(cursor, revision, chatVersion, chatSync);
    if (reply.snapshot.kind !== "task") throw new Error("expected a Task reply");
    // Fields are always current; the placeholder Chat is replaced by the client's own.
    reply.snapshot.task = {
      ...reply.snapshot.task,
      task: { ...taskSummary(), workspaceAvailable: false },
      historySync: { state: "reloadAvailable", generation: 3 },
      subagents: { totalCount: 2, runningCount: 1, attentionCount: 0, available: true },
    };
    return reply;
  }

  async function sessionWithReleasedTask(replies: StateSubscribeResult[]) {
    const params: unknown[] = [];
    const raw = fakeConnection(async (method, value) => {
      if (method === STATE_UNSUBSCRIBE) return { scope };
      if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
      params.push(value);
      const reply = replies.shift();
      if (!reply) throw new Error("No subscribe reply left");
      return reply;
    });
    const session = createAppServerSession(raw.connection);
    await session.initialize(initializeParams());
    const ready = vi.fn();
    const release = session.subscribeState(scope, { onSnapshot: vi.fn(), onBaselineReady: ready });
    await vi.waitFor(() => expect(ready).toHaveBeenCalledOnce());
    release();
    return { raw, session, params };
  }

  function taskSnapshots(session: ReturnType<typeof createAppServerSession>) {
    const snapshots: Array<Extract<StateSubscribeResult["snapshot"], { kind: "task" }>> = [];
    session.subscribeState(scope, {
      onSnapshot: (snapshot) => {
        if (snapshot.kind === "task") snapshots.push(snapshot as never);
      },
    });
    return snapshots;
  }

  it("offers the released copy's Chat version and installs the missed changes on its Chat", async () => {
    const { session, params } = await sessionWithReleasedTask([
      taskSubscription("cursor_1", 4, 10, { kind: "full" }, [userItem("one")]),
      resume(6, 12, { kind: "deltas", changes: [{ kind: "append", item: userItem("two") }] }),
    ]);

    const snapshots = taskSnapshots(session);

    await vi.waitFor(() => expect(snapshots).toHaveLength(1));
    expect(params).toEqual([{ scope }, { scope, knownVersion: 10 }]);
    const [snapshot] = snapshots;
    expect(snapshot.task.revision).toBe(6);
    expect(snapshot.chatVersion).toBe(12);
    expect(snapshot.task.chat.items.map((item) => item.messageId)).toEqual(["one", "two"]);
    // Fields come from the reply, not the retained copy.
    expect(snapshot.task.task.workspaceAvailable).toBe(false);
    expect(snapshot.task.historySync).toEqual({ state: "reloadAvailable", generation: 3 });
    expect(snapshot.task.subagents.totalCount).toBe(2);
    session.close();
  });

  it("keeps the retained Chat when the reply says it is current", async () => {
    const { session } = await sessionWithReleasedTask([
      taskSubscription("cursor_1", 4, 10, { kind: "full" }, [userItem("one")]),
      resume(5, 10, { kind: "current" }),
    ]);

    const snapshots = taskSnapshots(session);

    await vi.waitFor(() => expect(snapshots).toHaveLength(1));
    expect(snapshots[0].task.chat.items.map((item) => item.messageId)).toEqual(["one"]);
    expect(snapshots[0].chatVersion).toBe(10);
    session.close();
  });

  it("takes the reply's tail page when the server answers full", async () => {
    const { session } = await sessionWithReleasedTask([
      taskSubscription("cursor_1", 4, 10, { kind: "full" }, [userItem("one")]),
      taskSubscription("cursor_2", 9, 30, { kind: "full" }, [userItem("fresh")]),
    ]);

    const snapshots = taskSnapshots(session);

    await vi.waitFor(() => expect(snapshots).toHaveLength(1));
    expect(snapshots[0].task.chat.items.map((item) => item.messageId)).toEqual(["fresh"]);
    expect(snapshots[0].chatVersion).toBe(30);
    session.close();
  });

  it("subscribes again without a version when the missed changes do not fit the copy", async () => {
    const { session, params } = await sessionWithReleasedTask([
      taskSubscription("cursor_1", 4, 10),
      resume(6, 12, { kind: "deltas", changes: [{ kind: "appendText", messageId: "absent" as MessageId, text: "x" }] }),
      taskSubscription("cursor_2", 6, 12, { kind: "full" }, [userItem("fresh")]),
    ]);

    const snapshots = taskSnapshots(session);

    await vi.waitFor(() => expect(snapshots).toHaveLength(1));
    expect(params).toEqual([{ scope }, { scope, knownVersion: 10 }, { scope }]);
    expect(snapshots[0].task.chat.items.map((item) => item.messageId)).toEqual(["fresh"]);
    session.close();
  });

  it("offers a copy across a recovery on the same state root", async () => {
    const { raw, session, params } = await sessionWithReleasedTask([
      taskSubscription("cursor_1", 4, 10),
      resume(7, 10, { kind: "current" }),
    ]);
    raw.invalidate();
    raw.recover();

    const snapshots = taskSnapshots(session);

    await vi.waitFor(() => expect(snapshots).toHaveLength(1));
    expect(params).toEqual([{ scope }, { scope, knownVersion: 10 }]);
    session.close();
  });

  it("drops copies when a recovery reports another state root", async () => {
    const { raw, session, params } = await sessionWithReleasedTask([
      taskSubscription("cursor_1", 4, 10),
      taskSubscription("cursor_2", 7, 3),
    ]);
    raw.invalidate();
    const other = initializeResult();
    other.snapshot.stateRoot = { stateRootId: "root_2" as never };
    raw.recover(other);

    const snapshots = taskSnapshots(session);

    await vi.waitFor(() => expect(snapshots).toHaveLength(1));
    expect(params).toEqual([{ scope }, { scope }]);
    session.close();
  });
});

describe("AppServerSession Tool detail and Subagent History resume", () => {
  const toolScope = { kind: "toolDetail" as const, taskId: "task_1" as TaskId, artifactId: "artifact_1" };
  const historyScope = {
    kind: "subagentHistory" as const,
    taskId: "task_1" as TaskId,
    subagentId: "subagent_1" as SubagentId,
  };

  function toolReply(
    revision: number,
    terminalSync: ToolTerminalSync,
    terminalOutputs: Array<{ terminalId: string; output: string }> = [],
    content = "text",
  ): StateSubscribeResult {
    return {
      cursor: `cursor_${revision}` as EventCursor,
      scope: toolScope,
      snapshot: {
        kind: "toolDetail",
        taskId: toolScope.taskId,
        artifactId: toolScope.artifactId,
        terminalSync,
        details: { revision, locations: [], content: [{ kind: "text", text: content }], terminalOutputs },
      },
    };
  }

  function historyReply(
    revision: number,
    chatSync: SubagentChatSync,
    items: ChatItem[] = [],
    availability: "available" | "waitingForActivity" = "available",
  ): StateSubscribeResult {
    return {
      cursor: `cursor_${revision}` as EventCursor,
      scope: historyScope,
      snapshot: {
        kind: "subagentHistory",
        chatSync,
        history: {
          taskId: historyScope.taskId,
          subagentId: historyScope.subagentId,
          revision,
          availability,
          chat: { items, hasMoreBefore: false, hasMessages: items.length > 0 },
        },
      },
    };
  }

  async function released(scope: typeof toolScope | typeof historyScope, replies: StateSubscribeResult[]) {
    const params: unknown[] = [];
    const raw = fakeConnection(async (method, value) => {
      if (method === STATE_UNSUBSCRIBE) return { scope };
      if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
      params.push(value);
      const reply = replies.shift();
      if (!reply) throw new Error("No subscribe reply left");
      return reply;
    });
    const session = createAppServerSession(raw.connection);
    await session.initialize(initializeParams());
    const ready = vi.fn();
    const release = session.subscribeState(scope, { onSnapshot: vi.fn(), onBaselineReady: ready });
    await vi.waitFor(() => expect(ready).toHaveBeenCalledOnce());
    release();
    return { raw, session, params };
  }

  function snapshotsOf<K extends "toolDetail" | "subagentHistory">(
    session: ReturnType<typeof createAppServerSession>,
    scope: typeof toolScope | typeof historyScope,
    kind: K,
  ) {
    const snapshots: Array<Extract<import("./index").ReplicaSnapshot, { kind: K }>> = [];
    session.subscribeState(scope, {
      onSnapshot: (snapshot) => {
        if (snapshot.kind === kind) snapshots.push(snapshot as never);
      },
    });
    return snapshots;
  }

  it("offers the released Tool detail's artifact sequence and appends the missed output", async () => {
    const { session, params } = await released(toolScope, [
      toolReply(2, { kind: "full" }, [{ terminalId: "t1", output: "ab" }]),
      toolReply(4, { kind: "deltas", appends: [{ terminalId: "t1", data: "cd" }, { terminalId: "t2", data: "x" }] }, [], "newer"),
    ]);

    const snapshots = snapshotsOf(session, toolScope, "toolDetail");

    await vi.waitFor(() => expect(snapshots).toHaveLength(1));
    expect(params).toEqual([{ scope: toolScope }, { scope: toolScope, knownVersion: 2 }]);
    expect(snapshots[0].details.revision).toBe(4);
    expect(snapshots[0].details.terminalOutputs).toEqual([
      { terminalId: "t1", output: "abcd" },
      { terminalId: "t2", output: "x" },
    ]);
    // Fields come from the reply, not the retained copy.
    expect(snapshots[0].details.content).toEqual([{ kind: "text", text: "newer" }]);
    session.close();
  });

  it("keeps the retained terminal output when the Tool detail reply is current", async () => {
    const { session } = await released(toolScope, [
      toolReply(2, { kind: "full" }, [{ terminalId: "t1", output: "ab" }]),
      toolReply(2, { kind: "current" }),
    ]);

    const snapshots = snapshotsOf(session, toolScope, "toolDetail");

    await vi.waitFor(() => expect(snapshots).toHaveLength(1));
    expect(snapshots[0].details.terminalOutputs).toEqual([{ terminalId: "t1", output: "ab" }]);
    session.close();
  });

  it("takes the whole terminal output when the Tool detail reply is full", async () => {
    const { session } = await released(toolScope, [
      toolReply(2, { kind: "full" }, [{ terminalId: "t1", output: "ab" }]),
      toolReply(1, { kind: "full" }, [{ terminalId: "t1", output: "fresh" }]),
    ]);

    const snapshots = snapshotsOf(session, toolScope, "toolDetail");

    await vi.waitFor(() => expect(snapshots).toHaveLength(1));
    expect(snapshots[0].details.terminalOutputs).toEqual([{ terminalId: "t1", output: "fresh" }]);
    session.close();
  });

  it("offers a Tool detail copy across a recovery on the same state root", async () => {
    const { raw, session, params } = await released(toolScope, [
      toolReply(2, { kind: "full" }, [{ terminalId: "t1", output: "ab" }]),
      toolReply(2, { kind: "current" }),
    ]);
    raw.invalidate();
    raw.recover();

    const snapshots = snapshotsOf(session, toolScope, "toolDetail");

    await vi.waitFor(() => expect(snapshots).toHaveLength(1));
    expect(params).toEqual([{ scope: toolScope }, { scope: toolScope, knownVersion: 2 }]);
    session.close();
  });

  it("offers the released Subagent History's revision and keeps its Chat when current", async () => {
    const { session, params } = await released(historyScope, [
      historyReply(3, { kind: "full" }, [userItem("one")], "waitingForActivity"),
      historyReply(3, { kind: "current" }),
    ]);

    const snapshots = snapshotsOf(session, historyScope, "subagentHistory");

    await vi.waitFor(() => expect(snapshots).toHaveLength(1));
    expect(params).toEqual([{ scope: historyScope }, { scope: historyScope, knownVersion: 3 }]);
    expect(snapshots[0].history.chat.items.map((item) => item.messageId)).toEqual(["one"]);
    expect(snapshots[0].history.availability).toBe("available");
    session.close();
  });

  it("takes the tail page when the Subagent History reply is full", async () => {
    const { session } = await released(historyScope, [
      historyReply(3, { kind: "full" }, [userItem("one")]),
      historyReply(5, { kind: "full" }, [userItem("one"), userItem("two")]),
    ]);

    const snapshots = snapshotsOf(session, historyScope, "subagentHistory");

    await vi.waitFor(() => expect(snapshots).toHaveLength(1));
    expect(snapshots[0].history.revision).toBe(5);
    expect(snapshots[0].history.chat.items).toHaveLength(2);
    session.close();
  });

  it("evicts the least recently released Tool detail copy past its limit", async () => {
    const scopeFor = (index: number) => ({ ...toolScope, artifactId: `artifact_${index}` });
    const params: unknown[] = [];
    const raw = fakeConnection(async (method, value) => {
      if (method === STATE_UNSUBSCRIBE) return { scope: toolScope };
      if (method !== STATE_SUBSCRIBE) throw new Error(`Unexpected request: ${method}`);
      params.push(value);
      const scope = (value as { scope: typeof toolScope }).scope;
      return { ...toolReply(2, { kind: "full" }, [{ terminalId: "t1", output: "ab" }]), scope } as StateSubscribeResult;
    });
    const session = createAppServerSession(raw.connection);
    await session.initialize(initializeParams());
    for (let index = 0; index < 9; index += 1) {
      const ready = vi.fn();
      const release = session.subscribeState(scopeFor(index), { onSnapshot: vi.fn(), onBaselineReady: ready });
      await vi.waitFor(() => expect(ready).toHaveBeenCalledOnce());
      release();
    }
    params.length = 0;
    for (const index of [0, 8]) {
      const ready = vi.fn();
      session.subscribeState(scopeFor(index), { onSnapshot: vi.fn(), onBaselineReady: ready });
      await vi.waitFor(() => expect(ready).toHaveBeenCalledOnce());
    }

    expect(params).toEqual([{ scope: scopeFor(0) }, { scope: scopeFor(8), knownVersion: 2 }]);
    session.close();
  });
});
