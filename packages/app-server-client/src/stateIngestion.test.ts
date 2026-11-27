import { describe, expect, it } from "vitest";
import type {
  AppServerEvent,
  AppServerEventPayload,
  AgentId,
  ChatItem,
  EventCursor,
  ProjectId,
  StateRootId,
  StateSubscribeResult,
  SubscriptionScope,
  TaskId,
  TaskSnapshot,
  TaskSummary,
} from "./generated/protocol.js";
import { applySubscriptionEvent, createSubscriptionIngestionState } from "./stateIngestion.js";

const rootId = "root-1" as StateRootId;

describe("scope-local state ingestion", () => {
  it("applies live Agent collection status updates", () => {
    const state = createSubscriptionIngestionState({
      scope: { kind: "agents" },
      cursor: "cursor-1" as EventCursor,
      snapshot: {
        kind: "agents",
        agents: { agents: [{ agentId: "codex" as AgentId, label: "Codex", icon: "openai", status: "disconnected" }] },
      },
    }, {
      stateRootId: rootId,
      clientInstanceId: "client-1" as never,
    });
    const result = applySubscriptionEvent(state, {
      subscription: { kind: "agents" },
      previousCursor: "cursor-1" as EventCursor,
      cursor: "cursor-2" as EventCursor,
      scope: { kind: "stateRoot", stateRootId: rootId },
      payload: {
        kind: "agentCollectionUpdated",
        agents: { agents: [{ agentId: "codex" as AgentId, label: "Codex", icon: "openai", status: "installing" }] },
      },
    });

    expect(result).toMatchObject({
      kind: "applied",
      snapshotChanged: true,
      state: {
        snapshot: {
          kind: "agents",
          agents: { agents: [{ agentId: "codex", status: "installing" }] },
        },
      },
    });
  });

  it("replaces the Remote Device list from a live collection update", () => {
    const state = createSubscriptionIngestionState({
      scope: { kind: "devices" },
      cursor: "cursor-1" as EventCursor,
      snapshot: { kind: "devices", devices: { remoteAccess: "off", serverName: "workstation" } },
    }, {
      stateRootId: rootId,
      clientInstanceId: "client-1" as never,
    });
    const result = applySubscriptionEvent(state, {
      subscription: { kind: "devices" },
      previousCursor: "cursor-1" as EventCursor,
      cursor: "cursor-2" as EventCursor,
      scope: { kind: "stateRoot", stateRootId: rootId },
      payload: {
        kind: "deviceCollectionUpdated",
        devices: {
          remoteAccess: "on",
          serverName: "workstation",
          devices: [{ deviceId: "device-1", name: "Phone", addedAtMs: 1 }],
        },
      },
    });

    expect(result).toMatchObject({
      kind: "applied",
      snapshotChanged: true,
      state: {
        snapshot: {
          kind: "devices",
          devices: { remoteAccess: "on", devices: [{ deviceId: "device-1" }] },
        },
      },
    });
  });

  it("applies one atomic Task patch at the exact next Task revision", () => {
    const state = taskState("task-1", 4);
    const item = chatItem("agent-1", "Hello");
    const result = applySubscriptionEvent(state, taskEvent("task-1", "cursor-1", "cursor-2", {
      kind: "taskChanged",
      taskId: taskId("task-1"),
      revision: 5,
      changes: {
        task: { ...taskSummary("task-1"), status: "running", unread: true },
        activeTurnStartedAt: "2026-07-13T00:00:00Z",
        inputCapabilities: { image: true },
        contextUsage: {
          usedTokens: 31_000,
          capacityTokens: 258_400,
        },
        sendCapability: { state: "blocked", blockers: [] },
        chat: [{ kind: "append", item }],
        chatVersion: 9,
      },
    }));

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied" || result.state.snapshot.kind !== "task") return;
    // The Chat version the notification carries is the version a later resume offers.
    expect(result.state.snapshot.chatVersion).toBe(9);
    expect(result.state.snapshot.task).toMatchObject({
      revision: 5,
      task: { status: "running", unread: true },
      activeTurnStartedAt: "2026-07-13T00:00:00Z",
      inputCapabilities: { image: true },
      contextUsage: {
        usedTokens: 31_000,
        capacityTokens: 258_400,
      },
      sendCapability: { state: "blocked" },
    });
    expect(result.state.snapshot.task.chat.items).toEqual([item]);
  });

  it("keeps the Chat version when a notification changes no Chat", () => {
    const state = taskState("task-1", 4);
    const result = applySubscriptionEvent(state, taskEvent("task-1", "cursor-1", "cursor-2", {
      kind: "taskChanged",
      taskId: taskId("task-1"),
      revision: 5,
      changes: { task: { ...taskSummary("task-1"), unread: true } },
    }));

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied" || result.state.snapshot.kind !== "task") return;
    expect(result.state.snapshot.chatVersion).toBe(0);
  });

  it("follows the background command list through Task deltas", () => {
    const changed = (state: ReturnType<typeof taskState>, revision: number, changes: object) => {
      const result = applySubscriptionEvent(state, taskEvent("task-1", `cursor-${revision - 4}`, `cursor-${revision - 3}`, {
        kind: "taskChanged",
        taskId: taskId("task-1"),
        revision,
        changes,
      }));
      if (result.kind !== "applied" || result.state.snapshot.kind !== "task") throw new Error("delta was not applied");
      return result.state;
    };

    const command = {
      commandId: "command-1",
      description: "npm run build",
      startedAt: "2026-07-13T00:00:00Z",
      paused: false,
      canStop: true,
      stopFailed: false,
    };
    const held = changed(taskState("task-1", 4), 5, {
      task: { ...taskSummary("task-1"), status: "background" },
      backgroundCommands: [command],
    });
    expect(held.snapshot.kind === "task" && held.snapshot.task.backgroundCommands).toEqual([command]);

    // A delta that does not carry the Task summary leaves the count alone.
    const unrelated = changed(held, 6, { permissionPolicy: "autoApprove" });
    expect(unrelated.snapshot.kind === "task" && unrelated.snapshot.task.backgroundCommands).toEqual([command]);

    const running = changed(unrelated, 7, {
      task: { ...taskSummary("task-1"), status: "running" },
      backgroundCommands: [],
    });
    expect(running.snapshot.kind === "task" && running.snapshot.task.backgroundCommands).toEqual([]);
  });

  it("applies a Task-owned permission policy delta", () => {
    const state = taskState("task-1", 4);
    const result = applySubscriptionEvent(state, taskEvent("task-1", "cursor-1", "cursor-2", {
      kind: "taskChanged",
      taskId: taskId("task-1"),
      revision: 5,
      changes: { permissionPolicy: "autoApprove" },
    }));

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied" || result.state.snapshot.kind !== "task") return;
    expect(result.snapshotChanged).toBe(true);
    expect(result.state.snapshot.task.permissionPolicy).toBe("autoApprove");
  });

  it("clears stale context usage when a new live session removes it", () => {
    const state = taskState("task-1", 4);
    if (state.snapshot.kind !== "task") return;
    state.snapshot.task.contextUsage = {
      usedTokens: 31_000,
      capacityTokens: 258_400,
    };
    const result = applySubscriptionEvent(state, taskEvent("task-1", "cursor-1", "cursor-2", {
      kind: "taskChanged",
      taskId: taskId("task-1"),
      revision: 5,
      changes: { contextUsage: null },
    }));

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied" || result.state.snapshot.kind !== "task") return;
    expect(result.state.snapshot.task.contextUsage).toBeNull();
  });

  it("replaces and clears the current Agent Plan from ordered Task deltas", () => {
    const state = taskState("task-1", 4);
    const plan = {
      entries: [{ content: "Implement", priority: "high" as const, status: "inProgress" as const }],
    };
    const replaced = applySubscriptionEvent(state, taskEvent("task-1", "cursor-1", "cursor-2", {
      kind: "taskChanged",
      taskId: taskId("task-1"),
      revision: 5,
      changes: { currentPlan: plan },
    }));
    expect(replaced.kind).toBe("applied");
    if (replaced.kind !== "applied" || replaced.state.snapshot.kind !== "task") return;
    expect(replaced.state.snapshot.task.currentPlan).toEqual(plan);

    const cleared = applySubscriptionEvent(
      replaced.state,
      taskEvent("task-1", "cursor-2", "cursor-3", {
        kind: "taskChanged",
        taskId: taskId("task-1"),
        revision: 6,
        changes: { currentPlan: null },
      }),
    );
    expect(cleared.kind).toBe("applied");
    if (cleared.kind !== "applied" || cleared.state.snapshot.kind !== "task") return;
    expect(cleared.state.snapshot.task.currentPlan).toBeNull();
  });

  it("removes a delivered Queue head from an ordered Task delta", () => {
    const state = taskState("task-1", 4);
    if (state.snapshot.kind !== "task") return;
    state.snapshot.task.messageQueue = {
      revision: 2,
      items: [{
        queuedMessageId: "queued-1" as never,
        text: "Send after this turn",
        createdAt: "2026-08-05T00:00:00Z",
      }],
    };

    const result = applySubscriptionEvent(state, taskEvent("task-1", "cursor-1", "cursor-2", {
      kind: "taskChanged",
      taskId: taskId("task-1"),
      revision: 5,
      changes: { messageQueue: { revision: 3, items: [] } },
    }));

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied" || result.state.snapshot.kind !== "task") return;
    expect(result.state.snapshot.task.messageQueue).toEqual({ revision: 3, items: [] });
  });

  it("requires a fresh baseline for a missing Task revision", () => {
    const result = applySubscriptionEvent(
      taskState("task-1", 4),
      taskEvent("task-1", "cursor-1", "cursor-2", {
        kind: "taskChanged",
        taskId: taskId("task-1"),
        revision: 6,
        changes: {},
      }),
    );
    expect(result).toMatchObject({ kind: "resyncRequired", reason: "taskRevisionGap" });
  });

  it("ignores an already represented Task revision", () => {
    const state = taskState("task-1", 4);
    const result = applySubscriptionEvent(state, taskEvent("task-1", "cursor-1", "cursor-2", {
      kind: "taskChanged",
      taskId: taskId("task-1"),
      revision: 4,
      changes: { task: { ...taskSummary("task-1"), status: "running" } },
    }));
    expect(result).toMatchObject({ kind: "applied", snapshotChanged: false });
  });

  it("ignores another subscription without advancing this cursor", () => {
    const state = taskState("task-1", 4);
    const event: AppServerEvent = {
      subscription: { kind: "agents" },
      previousCursor: cursor("cursor-90"),
      cursor: cursor("cursor-91"),
      scope: { kind: "stateRoot", stateRootId: rootId },
      payload: { kind: "agentCollectionUpdated", agents: { agents: [] } },
    };
    const result = applySubscriptionEvent(state, event);
    expect(result).toMatchObject({ kind: "ignored", reason: "subscriptionMismatch" });
    expect(result.state.cursor).toBe("cursor-1");
  });

  it("requires a baseline when its own scope cursor skips", () => {
    const result = applySubscriptionEvent(
      taskState("task-1", 4),
      taskEvent("task-1", "cursor-missing", "cursor-3", {
        kind: "taskChanged",
        taskId: taskId("task-1"),
        revision: 5,
        changes: {},
      }),
    );
    expect(result).toMatchObject({ kind: "resyncRequired", reason: "cursorGap" });
  });

  it("applies an ordered Chat operation batch before exposing the replica", () => {
    const result = applySubscriptionEvent(
      taskState("task-1", 1),
      taskEvent("task-1", "cursor-1", "cursor-2", {
        kind: "taskChanged",
        taskId: taskId("task-1"),
        revision: 2,
        changes: {
          chat: [
            { kind: "append", item: chatItem("agent-1", "First") },
            { kind: "appendText", messageId: "agent-1" as never, text: " second" },
            { kind: "append", item: chatItem("thought-1", "Thinking", "system") },
          ],
        },
      }),
    );
    expect(result.kind).toBe("applied");
    if (result.kind !== "applied" || result.state.snapshot.kind !== "task") return;
    expect(result.state.snapshot.task.chat.items[0]?.parts).toEqual([{ kind: "text", text: "First second" }]);
    expect(result.state.snapshot.task.chat.items[1]?.messageId).toBe("thought-1");
  });

  it("requires resync when appended text has no base Chat item", () => {
    const result = applySubscriptionEvent(
      taskState("task-1", 1),
      taskEvent("task-1", "cursor-1", "cursor-2", {
        kind: "taskChanged",
        taskId: taskId("task-1"),
        revision: 2,
        changes: { chat: [{ kind: "appendText", messageId: "missing" as never, text: "x" }] },
      }),
    );
    expect(result).toMatchObject({ kind: "resyncRequired", reason: "missingChatItem" });
  });

  it("replaces Chat atomically for explicit history replacement", () => {
    const replacement = {
      items: [chatItem("history-1", "Loaded")],
      hasMoreBefore: false,
      hasMessages: true,
    };
    const result = applySubscriptionEvent(
      taskState("task-1", 1),
      taskEvent("task-1", "cursor-1", "cursor-2", {
        kind: "taskChanged",
        taskId: taskId("task-1"),
        revision: 2,
        changes: { chat: [{ kind: "replace", chat: replacement }] },
      }),
    );
    expect(result.kind).toBe("applied");
    if (result.kind !== "applied" || result.state.snapshot.kind !== "task") return;
    expect(result.state.snapshot.task.chat).toEqual(replacement);
  });

  it("applies process-local history state without consuming Task revision", () => {
    const result = applySubscriptionEvent(
      taskState("task-1", 7),
      taskEvent("task-1", "cursor-1", "cursor-2", {
        kind: "taskHistorySyncUpdated",
        taskId: taskId("task-1"),
        historySync: { state: "syncing", generation: 3 },
      }),
    );
    expect(result.kind).toBe("applied");
    if (result.kind !== "applied" || result.state.snapshot.kind !== "task") return;
    expect(result.state.snapshot.task.revision).toBe(7);
    expect(result.state.snapshot.task.historySync).toEqual({ state: "syncing", generation: 3 });
  });

  it("applies terminal deltas only to the matching Tool-detail replica", () => {
    const scope: SubscriptionScope = {
      kind: "toolDetail",
      taskId: taskId("task-1"),
      artifactId: "artifact-1",
    };
    const state = createSubscriptionIngestionState({
      scope,
      cursor: cursor("cursor-1"),
      snapshot: {
        kind: "toolDetail",
        taskId: taskId("task-1"),
        artifactId: "artifact-1",
        details: { revision: 0, locations: [], content: [], terminalOutputs: [{ terminalId: "term-1", output: "a" }] },
      },
    }, context());
    const event: AppServerEvent = {
      subscription: scope,
      previousCursor: cursor("cursor-1"),
      cursor: cursor("cursor-2"),
      scope: { kind: "task", stateRootId: rootId, taskId: taskId("task-1") },
      payload: {
        kind: "toolDetailChanged",
        taskId: taskId("task-1"),
        artifactId: "artifact-1",
        revision: 1,
        deltas: [
          { kind: "appendTerminal", terminalId: "term-1", data: "b" },
          { kind: "appendTerminal", terminalId: "term-2", data: "c" },
        ],
      },
    };

    const result = applySubscriptionEvent(state, event);

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied" || result.state.snapshot.kind !== "toolDetail") return;
    expect(result.state.snapshot.details.terminalOutputs).toEqual([
      { terminalId: "term-1", output: "ab" },
      { terminalId: "term-2", output: "c" },
    ]);

    const duplicate = applySubscriptionEvent(result.state, {
      ...event,
      previousCursor: cursor("cursor-2"),
      cursor: cursor("cursor-3"),
    });
    expect(duplicate.kind).toBe("applied");
    if (duplicate.kind !== "applied" || duplicate.state.snapshot.kind !== "toolDetail") return;
    expect(duplicate.state.snapshot.details.terminalOutputs).toEqual([
      { terminalId: "term-1", output: "ab" },
      { terminalId: "term-2", output: "c" },
    ]);
  });

  it("requires a fresh Tool-detail baseline when an artifact revision is skipped", () => {
    const state = toolDetailState(0);
    const result = applySubscriptionEvent(state, toolDetailEvent("cursor-1", "cursor-2", {
      kind: "toolDetailChanged",
      taskId: taskId("task-1"),
      artifactId: "artifact-1",
      revision: 2,
      deltas: [{ kind: "appendTerminal", terminalId: "term-1", data: "lost-prefix" }],
    }));

    expect(result).toMatchObject({ kind: "resyncRequired", reason: "toolDetailRevisionGap" });
  });

  it("skips a Tool-detail notification the installed baseline already contains", () => {
    const state = toolDetailState(1);
    const result = applySubscriptionEvent(state, toolDetailEvent("cursor-1", "cursor-2", {
      kind: "toolDetailChanged",
      taskId: taskId("task-1"),
      artifactId: "artifact-1",
      revision: 1,
      deltas: [{ kind: "appendTerminal", terminalId: "term-1", data: "already-in-baseline" }],
    }));

    expect(result).toMatchObject({ kind: "applied", snapshotChanged: false });
  });

  it("applies the fields and terminal appends of one Tool-detail commit together", () => {
    const state = toolDetailState(1);
    const result = applySubscriptionEvent(state, toolDetailEvent("cursor-1", "cursor-2", {
      kind: "toolDetailChanged",
      taskId: taskId("task-1"),
      artifactId: "artifact-1",
      revision: 2,
      deltas: [
        {
          kind: "replaceDetails",
          details: { revision: 2, locations: [], content: [{ kind: "text", text: "done" }], terminalOutputs: [] },
        },
        { kind: "appendTerminal", terminalId: "term-1", data: "out" },
      ],
    }));

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied" || result.state.snapshot.kind !== "toolDetail") return;
    expect(result.state.snapshot.details).toEqual({
      revision: 2,
      locations: [],
      content: [{ kind: "text", text: "done" }],
      terminalOutputs: [{ terminalId: "term-1", output: "out" }],
    });
  });

  it.each([
    ["toolDetail", { kind: "toolDetail", taskId: taskId("task-1"), artifactId: "artifact-1" }],
    ["subagentHistory", { kind: "subagentHistory", taskId: taskId("task-1"), subagentId: "subagent-1" as never }],
    ["subagentCatalog", { kind: "subagentCatalog", taskId: taskId("task-1") }],
  ] as const)("hands a %s subscription its Task's removal and ignores another Task's", (_, scope) => {
    const state = createSubscriptionIngestionState({
      scope: scope as SubscriptionScope,
      cursor: cursor("cursor-1"),
      snapshot: scope.kind === "toolDetail"
        ? { kind: "toolDetail", taskId: taskId("task-1"), artifactId: "artifact-1", details: { revision: 1, locations: [], content: [] } }
        : scope.kind === "subagentHistory"
          ? {
            kind: "subagentHistory",
            history: {
              taskId: taskId("task-1"),
              subagentId: "subagent-1" as never,
              revision: 1,
              availability: "available",
              chat: { items: [], hasMoreBefore: false, hasMessages: false },
            },
          }
          : { kind: "subagentCatalog", catalog: { taskId: taskId("task-1"), revision: 1, entries: [] } },
    }, context());
    const event = (id: string): AppServerEvent => ({
      subscription: scope as SubscriptionScope,
      previousCursor: cursor("cursor-1"),
      cursor: cursor("cursor-2"),
      scope: { kind: "task", stateRootId: rootId, taskId: taskId(id) },
      payload: { kind: "taskContentRemoved", taskId: taskId(id) },
    });

    expect(applySubscriptionEvent(state, event("task-1"))).toMatchObject({ kind: "applied" });
    expect(applySubscriptionEvent(state, event("task-2"))).toMatchObject({ kind: "ignored" });
  });
});

function toolDetailState(revision: number) {
  const scope: SubscriptionScope = {
    kind: "toolDetail",
    taskId: taskId("task-1"),
    artifactId: "artifact-1",
  };
  return createSubscriptionIngestionState({
    scope,
    cursor: cursor("cursor-1"),
    snapshot: {
      kind: "toolDetail",
      taskId: taskId("task-1"),
      artifactId: "artifact-1",
      details: { revision, locations: [], content: [], terminalOutputs: [] },
    },
  }, context());
}

function toolDetailEvent(
  previous: string,
  next: string,
  payload: AppServerEventPayload,
): AppServerEvent {
  const subscription: SubscriptionScope = {
    kind: "toolDetail",
    taskId: taskId("task-1"),
    artifactId: "artifact-1",
  };
  return {
    subscription,
    previousCursor: cursor(previous),
    cursor: cursor(next),
    scope: { kind: "task", stateRootId: rootId, taskId: taskId("task-1") },
    payload,
  };
}

function taskState(id: string, revision: number) {
  const scope: SubscriptionScope = { kind: "task", taskId: taskId(id) };
  return createSubscriptionIngestionState({
    scope,
    cursor: cursor("cursor-1"),
    snapshot: { kind: "task", task: taskSnapshot(id, revision), chatVersion: 0 },
  }, context());
}

function taskEvent(
  id: string,
  previous: string,
  next: string,
  payload: AppServerEventPayload,
): AppServerEvent {
  const subscription: SubscriptionScope = { kind: "task", taskId: taskId(id) };
  return {
    subscription,
    previousCursor: cursor(previous),
    cursor: cursor(next),
    scope: { kind: "task", stateRootId: rootId, taskId: taskId(id) },
    payload,
  };
}

function taskSnapshot(id: string, revision: number): TaskSnapshot {
  return {
    task: taskSummary(id),
    lifecycle: "open",
    revision,
    permissionPolicy: "askEveryTime",
    preparation: { kind: "ready" },
    agentConfig: { state: "ready", options: [] },
    agentCommands: { state: "ready", commands: [] },
    sendCapability: { state: "ready", blockers: [] },
    messageQueue: { revision: 0, items: [] },
    chat: { items: [], hasMoreBefore: false, hasMessages: false },
    historySync: { state: "idle", generation: 0 },
    subagents: { totalCount: 0, runningCount: 0, attentionCount: 0, available: true },
    pendingRequests: [],
  };
}

function taskSummary(id: string): TaskSummary {
  return {
    taskId: taskId(id),
    projectId: "project-1" as never,
    agentId: "agent-1" as never,
    lifecycle: "open",
    title: { value: "Task", source: "user" },
    status: "idle",
    updatedAt: "1",
    lastActivity: "1",
    unread: false,
    hasMessages: false,
    workspaceAvailable: true,
  };
}

function chatItem(id: string, text: string, role: ChatItem["role"] = "agent"): ChatItem {
  return {
    messageId: id as never,
    role,
    status: "complete",
    parts: [{ kind: "text", text }],
  };
}

function context() {
  return { stateRootId: rootId, clientInstanceId: "client-1" as never };
}

function taskId(value: string): TaskId {
  return value as TaskId;
}

function cursor(value: string): EventCursor {
  return value as EventCursor;
}
