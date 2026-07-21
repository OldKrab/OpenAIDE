import { describe, expect, it } from "vitest";
import type {
  AppServerEvent,
  AppServerEventPayload,
  ChatItem,
  EventCursor,
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
        sendCapability: { state: "blocked", blockers: [] },
        chat: [{ kind: "append", item }],
      },
    }));

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied" || result.state.snapshot.kind !== "task") return;
    expect(result.state.snapshot.task).toMatchObject({
      revision: 5,
      task: { status: "running", unread: true },
      activeTurnStartedAt: "2026-07-13T00:00:00Z",
      inputCapabilities: { image: true },
      sendCapability: { state: "blocked" },
    });
    expect(result.state.snapshot.task.chat.items).toEqual([item]);
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

  it("applies focused Navigation upserts and removals", () => {
    const scope: SubscriptionScope = { kind: "taskNavigation", projectId: null };
    let state = createSubscriptionIngestionState({
      scope,
      cursor: cursor("cursor-1"),
      snapshot: { kind: "taskNavigation", navigation: { tasks: [], activeTaskId: null } },
    }, context());
    const upsert = applySubscriptionEvent(state, navigationEvent(scope, "cursor-1", "cursor-2", {
      kind: "upsert",
      task: taskSummary("task-1"),
    }));
    expect(upsert.kind).toBe("applied");
    if (upsert.kind !== "applied") return;
    state = upsert.state;
    const remove = applySubscriptionEvent(state, navigationEvent(scope, "cursor-2", "cursor-3", {
      kind: "remove",
      taskId: taskId("task-1"),
    }));
    expect(remove.kind).toBe("applied");
    if (remove.kind !== "applied" || remove.state.snapshot.kind !== "taskNavigation") return;
    expect(remove.state.snapshot.navigation.tasks).toEqual([]);
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

  it.each(["baseline-first", "delta-first"] as const)(
    "keeps mixed structured and terminal output when the %s event arrives",
    (order) => {
      const baseline: AppServerEventPayload = {
        kind: "toolDetailUpdated",
        taskId: taskId("task-1"),
        artifactId: "artifact-1",
        details: {
          revision: 1,
          locations: [],
          content: [],
          terminalOutputs: [{ terminalId: "term-1", output: "complete" }],
        },
      };
      const delta: AppServerEventPayload = {
        kind: "toolDetailChanged",
        taskId: taskId("task-1"),
        artifactId: "artifact-1",
        revision: 1,
        deltas: [
          {
            kind: "replaceDetails",
            details: { revision: 1, locations: [], content: [], terminalOutputs: [] },
          },
          { kind: "appendTerminal", terminalId: "term-1", data: "complete" },
        ],
      };
      const payloads = order === "baseline-first" ? [baseline, delta] : [delta, baseline];
      let state = toolDetailState(0);
      for (const [index, payload] of payloads.entries()) {
        const result = applySubscriptionEvent(
          state,
          toolDetailEvent(`cursor-${index + 1}`, `cursor-${index + 2}`, payload),
        );
        expect(result.kind).toBe("applied");
        if (result.kind !== "applied") return;
        state = result.state;
      }
      expect(state.snapshot.kind).toBe("toolDetail");
      if (state.snapshot.kind !== "toolDetail") return;
      expect(state.snapshot.details.terminalOutputs).toEqual([
        { terminalId: "term-1", output: "complete" },
      ]);
    },
  );
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
    snapshot: { kind: "task", task: taskSnapshot(id, revision) },
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

function navigationEvent(
  subscription: Extract<SubscriptionScope, { kind: "taskNavigation" }>,
  previous: string,
  next: string,
  change: Extract<AppServerEventPayload, { kind: "taskNavigationChanged" }>["change"],
): AppServerEvent {
  return {
    subscription,
    previousCursor: cursor(previous),
    cursor: cursor(next),
    scope: { kind: "stateRoot", stateRootId: rootId },
    payload: { kind: "taskNavigationChanged", change },
  };
}

function taskSnapshot(id: string, revision: number): TaskSnapshot {
  return {
    task: taskSummary(id),
    lifecycle: "visible",
    revision,
    preparation: { kind: "ready" },
    agentConfig: { state: "ready", options: [] },
    agentCommands: { state: "ready", commands: [] },
    sendCapability: { state: "ready", blockers: [] },
    chat: { items: [], hasMoreBefore: false, hasMessages: false },
    historySync: { state: "idle", generation: 0 },
    pendingRequests: [],
  };
}

function taskSummary(id: string): TaskSummary {
  return {
    taskId: taskId(id),
    projectId: "project-1" as never,
    agentId: "agent-1" as never,
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
