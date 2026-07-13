import { describe, expect, it } from "vitest";

import type {
  AppServerEvent,
  AgentId,
  ChatItem,
  ClientInstanceId,
  ClientSnapshot,
  EventCursor,
  MessageId,
  PendingRequestSnapshot,
  ProjectId,
  RequestId,
  ServerId,
  StateRootId,
  StateSubscribeResult,
  SubscriptionScope,
  SubscriptionSnapshot,
  TaskId,
  TaskNavigationSnapshot,
  TaskSnapshot,
  TaskSummary,
} from "./generated/protocol.js";
import {
  applySubscriptionEvent,
  createSubscriptionIngestionState,
} from "./stateIngestion.js";
import { subscriptionScopesEqual } from "./subscriptionScope.js";

describe("state ingestion", () => {
  it("applies an in-scope task event when the previous cursor matches", () => {
    const state = createSubscriptionIngestionState(subscribeResult(taskScope("task-1"), taskSnapshot("task-1"), "cursor-1"), {
      stateRootId: stateRoot("root-1"),
    });
    const event = taskEvent("root-1", "task-1", "cursor-1", "cursor-2", {
      kind: "chatItemAppended",
      taskId: taskId("task-1"),
      revision: 2,
      item: chatItem("message-1", "Hello"),
    });

    const result = applySubscriptionEvent(state, event);

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied") throw new Error("expected applied event");
    expect(result.state.cursor).toBe("cursor-2");
    expect(result.snapshotChanged).toBe(true);
    expect(result.state.snapshot.kind).toBe("task");
    if (result.state.snapshot.kind !== "task") throw new Error("expected task snapshot");
    expect(result.state.snapshot.task.chat.items).toHaveLength(1);
  });

  it("requires resync when an in-scope event skips the current cursor", () => {
    const state = createSubscriptionIngestionState(subscribeResult(taskScope("task-1"), taskSnapshot("task-1"), "cursor-1"), {
      stateRootId: stateRoot("root-1"),
    });
    const event = taskEvent("root-1", "task-1", "cursor-x", "cursor-2", {
      kind: "taskUpdated",
      task: taskSummary("task-1"),
    });

    const result = applySubscriptionEvent(state, event);

    expect(result).toMatchObject({ kind: "resyncRequired", reason: "cursorGap" });
  });

  it("ignores events for another active subscription while advancing the stream cursor", () => {
    const state = createSubscriptionIngestionState(subscribeResult(taskScope("task-1"), taskSnapshot("task-1"), "cursor-1"), {
      stateRootId: stateRoot("root-1"),
    });
    const event = taskEvent("root-1", "task-2", "cursor-1", "cursor-2", {
      kind: "taskUpdated",
      task: taskSummary("task-2"),
    });

    const result = applySubscriptionEvent(state, event);

    expect(result).toMatchObject({ kind: "ignored", reason: "subscriptionMismatch" });
    expect(result.state.cursor).toBe("cursor-2");
  });

  it("requires resync when an out-of-scope event exposes a cursor gap", () => {
    const state = createSubscriptionIngestionState(
      subscribeResult({ kind: "agents" }, { kind: "agents", agents: { agents: [] } }, "cursor-504"),
      { stateRootId: stateRoot("root-1") },
    );
    const event: AppServerEvent = {
      previousCursor: eventCursor("cursor-506"),
      cursor: eventCursor("cursor-507"),
      scope: { kind: "stateRoot", stateRootId: stateRoot("root-1") },
      payload: { kind: "taskUpdated", task: taskSummary("task-1") },
    };

    const result = applySubscriptionEvent(state, event);

    expect(result).toMatchObject({ kind: "resyncRequired", reason: "cursorGap" });
    expect(result.state.cursor).toBe("cursor-504");
  });

  it("requires resync when a state-root replacement exposes a cursor gap", () => {
    const state = createSubscriptionIngestionState(
      subscribeResult(
        { kind: "projects" },
        { kind: "projects", projects: { projects: [] } },
        "cursor-1",
      ),
      { stateRootId: stateRoot("root-1") },
    );
    const event = projectCollectionEvent("root-1", "cursor-missing", "cursor-3");

    const result = applySubscriptionEvent(state, event);

    expect(result).toMatchObject({ kind: "resyncRequired", reason: "cursorGap" });
    expect(result.state.cursor).toBe("cursor-1");
  });

  it("requires resync for events from another state root", () => {
    const state = createSubscriptionIngestionState(subscribeResult(taskScope("task-1"), taskSnapshot("task-1"), "cursor-1"), {
      stateRootId: stateRoot("root-1"),
    });
    const event = taskEvent("root-2", "task-1", "cursor-1", "cursor-2", {
      kind: "taskUpdated",
      task: taskSummary("task-1"),
    });

    const result = applySubscriptionEvent(state, event);

    expect(result).toMatchObject({ kind: "resyncRequired", reason: "streamScopeMismatch" });
  });

  it("requires resync for events from another initialized client", () => {
    const state = createSubscriptionIngestionState(
      subscribeResult({ kind: "settings" }, { kind: "settings", settings: { sections: [] } }, "cursor-1"),
      {
        stateRootId: stateRoot("root-1"),
        clientInstanceId: clientInstance("client-1"),
      },
    );
    const event: AppServerEvent = {
      previousCursor: eventCursor("cursor-1"),
      cursor: eventCursor("cursor-2"),
      scope: { kind: "client", stateRootId: stateRoot("root-1"), clientInstanceId: clientInstance("client-2") },
      payload: { kind: "snapshotReplaced", snapshot: clientSnapshot("root-1", "client-2") },
    };

    const result = applySubscriptionEvent(state, event);

    expect(result).toMatchObject({ kind: "resyncRequired", reason: "streamScopeMismatch" });
  });

  it("ignores task navigation updates outside a project filter while advancing the stream cursor", () => {
    const scope: SubscriptionScope = { kind: "taskNavigation", projectId: projectId("project-1") };
    const snapshot: SubscriptionSnapshot = {
      kind: "taskNavigation",
      navigation: { tasks: [], activeTaskId: null },
    };
    const state = createSubscriptionIngestionState(subscribeResult(scope, snapshot, "cursor-1"), {
      stateRootId: stateRoot("root-1"),
    });
    const event: AppServerEvent = {
      previousCursor: eventCursor("cursor-1"),
      cursor: eventCursor("cursor-2"),
      scope: { kind: "stateRoot", stateRootId: stateRoot("root-1") },
      payload: { kind: "taskUpdated", task: { ...taskSummary("task-2"), projectId: projectId("project-2") } },
    };

    const result = applySubscriptionEvent(state, event);

    expect(result).toMatchObject({ kind: "ignored", reason: "subscriptionMismatch" });
    expect(result.state.cursor).toBe("cursor-2");
  });

  it("does not resync after ignored same-stream events before a matching event", () => {
    const state = createSubscriptionIngestionState(
      subscribeResult({ kind: "projects" }, { kind: "projects", projects: { projects: [] } }, "cursor-1"),
      { stateRootId: stateRoot("root-1") },
    );
    const taskEvent: AppServerEvent = {
      previousCursor: eventCursor("cursor-1"),
      cursor: eventCursor("cursor-2"),
      scope: { kind: "stateRoot", stateRootId: stateRoot("root-1") },
      payload: { kind: "taskUpdated", task: taskSummary("task-1") },
    };
    const projectEvent = projectCollectionEvent("root-1", "cursor-2", "cursor-3");

    const ignored = applySubscriptionEvent(state, taskEvent);
    expect(ignored.kind).toBe("ignored");

    const result = applySubscriptionEvent(ignored.state, projectEvent);

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied") throw new Error("expected project update");
    expect(result.state.cursor).toBe("cursor-3");
  });

  it("filters task navigation replacement events by project scope", () => {
    const scope: SubscriptionScope = { kind: "taskNavigation", projectId: projectId("project-1") };
    const state = createSubscriptionIngestionState(
      subscribeResult(scope, { kind: "taskNavigation", navigation: taskNavigation([]) }, "cursor-1"),
      { stateRootId: stateRoot("root-1") },
    );
    const event: AppServerEvent = {
      previousCursor: eventCursor("cursor-1"),
      cursor: eventCursor("cursor-2"),
      scope: { kind: "stateRoot", stateRootId: stateRoot("root-1") },
      payload: {
        kind: "taskNavigationUpdated",
        navigation: taskNavigation([
          taskSummary("task-1", "project-1"),
          taskSummary("task-2", "project-2"),
        ], "task-2"),
      },
    };

    const result = applySubscriptionEvent(state, event);

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied") throw new Error("expected applied navigation replacement");
    expect(result.state.snapshot.kind).toBe("taskNavigation");
    if (result.state.snapshot.kind !== "taskNavigation") throw new Error("expected task navigation snapshot");
    expect(result.state.snapshot.navigation.tasks.map((task) => task.taskId)).toEqual([taskId("task-1")]);
    expect(result.state.snapshot.navigation.activeTaskId).toBeNull();
  });

  it("filters task navigation snapshot replacements by project scope", () => {
    const scope: SubscriptionScope = { kind: "taskNavigation", projectId: projectId("project-1") };
    const state = createSubscriptionIngestionState(
      subscribeResult(scope, { kind: "taskNavigation", navigation: taskNavigation([]) }, "cursor-1"),
      { stateRootId: stateRoot("root-1") },
    );
    const event: AppServerEvent = {
      previousCursor: eventCursor("cursor-1"),
      cursor: eventCursor("cursor-2"),
      scope: { kind: "stateRoot", stateRootId: stateRoot("root-1") },
      payload: {
        kind: "snapshotReplaced",
        snapshot: clientSnapshot("root-1", "client-1", {
          tasks: taskNavigation([
            taskSummary("task-1", "project-1"),
            taskSummary("task-2", "project-2"),
          ], "task-1"),
        }),
      },
    };

    const result = applySubscriptionEvent(state, event);

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied") throw new Error("expected applied snapshot replacement");
    expect(result.state.snapshot.kind).toBe("taskNavigation");
    if (result.state.snapshot.kind !== "taskNavigation") throw new Error("expected task navigation snapshot");
    expect(result.state.snapshot.navigation.tasks.map((task) => task.taskId)).toEqual([taskId("task-1")]);
    expect(result.state.snapshot.navigation.activeTaskId).toBe(taskId("task-1"));
  });

  it("applies task text chunks to the existing chat item", () => {
    const snapshot = taskSnapshot("task-1", [chatItem("message-1", "Hel")]);
    const state = createSubscriptionIngestionState(subscribeResult(taskScope("task-1"), snapshot, "cursor-1"), {
      stateRootId: stateRoot("root-1"),
    });
    const event = taskEvent("root-1", "task-1", "cursor-1", "cursor-2", {
      kind: "chatItemChunk",
      taskId: taskId("task-1"),
      revision: 2,
      messageId: messageId("message-1"),
      chunk: { sequence: 1, text: "lo", finalChunk: true },
    });

    const result = applySubscriptionEvent(state, event);

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied") throw new Error("expected applied chunk");
    expect(result.state.snapshot.kind).toBe("task");
    if (result.state.snapshot.kind !== "task") throw new Error("expected task snapshot");
    expect(result.state.snapshot.task.chat.items[0]?.parts).toEqual([{ kind: "text", text: "Hello" }]);
    expect(result.state.snapshot.task.chat.items[0]?.status).toBe("complete");
  });

  it("advances the task revision across committed append and chunk deltas", () => {
    const snapshot = taskSnapshot("task-1");
    if (snapshot.kind !== "task") throw new Error("expected task snapshot");
    snapshot.task.revision = 8;
    const state = createSubscriptionIngestionState(subscribeResult(taskScope("task-1"), snapshot, "cursor-1"), {
      stateRootId: stateRoot("root-1"),
    });
    const appendPayload = {
      kind: "chatItemAppended" as const,
      taskId: taskId("task-1"),
      revision: 9,
      item: chatItem("message-1", "Hel"),
    };
    const appendEvent = taskEvent("root-1", "task-1", "cursor-1", "cursor-2", appendPayload);

    const appended = applySubscriptionEvent(state, appendEvent);

    expect(appended.kind).toBe("applied");
    if (appended.kind !== "applied" || appended.state.snapshot.kind !== "task") {
      throw new Error("expected applied append");
    }
    expect(appended.state.snapshot.task.revision).toBe(9);

    const chunkPayload = {
      kind: "chatItemChunk" as const,
      taskId: taskId("task-1"),
      revision: 10,
      messageId: messageId("message-1"),
      chunk: { sequence: 1, text: "lo", finalChunk: true },
    };
    const chunkEvent = taskEvent("root-1", "task-1", "cursor-2", "cursor-3", chunkPayload);
    const chunked = applySubscriptionEvent(appended.state, chunkEvent);

    expect(chunked.kind).toBe("applied");
    if (chunked.kind !== "applied" || chunked.state.snapshot.kind !== "task") {
      throw new Error("expected applied chunk");
    }
    expect(chunked.state.snapshot.task.revision).toBe(10);
    expect(chunked.state.snapshot.task.chat.items[0]?.parts).toEqual([{ kind: "text", text: "Hello" }]);
  });

  it("does not replay a text delta already represented by the snapshot revision", () => {
    const snapshot = taskSnapshot("task-1", [chatItem("message-1", "Hello")]);
    if (snapshot.kind !== "task") throw new Error("expected task snapshot");
    snapshot.task.revision = 2;
    const state = createSubscriptionIngestionState(subscribeResult(taskScope("task-1"), snapshot, "cursor-1"), {
      stateRootId: stateRoot("root-1"),
    });
    const event = taskEvent("root-1", "task-1", "cursor-1", "cursor-2", {
      kind: "chatItemChunk",
      taskId: taskId("task-1"),
      revision: 2,
      messageId: messageId("message-1"),
      chunk: { sequence: 1, text: "lo", finalChunk: false },
    });

    const result = applySubscriptionEvent(state, event);

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied" || result.state.snapshot.kind !== "task") {
      throw new Error("expected already represented delta to advance only the cursor");
    }
    expect(result.snapshotChanged).toBe(false);
    expect(result.state.snapshot.task.chat.items[0]?.parts).toEqual([{ kind: "text", text: "Hello" }]);
    expect(result.state.cursor).toBe("cursor-2");
  });

  it("requires resync when a text chunk targets a missing chat item", () => {
    const snapshot = taskSnapshot("task-1");
    const state = createSubscriptionIngestionState(subscribeResult(taskScope("task-1"), snapshot, "cursor-1"), {
      stateRootId: stateRoot("root-1"),
    });
    const event = taskEvent("root-1", "task-1", "cursor-1", "cursor-2", {
      kind: "chatItemChunk",
      taskId: taskId("task-1"),
      revision: 2,
      messageId: messageId("missing-message"),
      chunk: { sequence: 1, text: "orphan", finalChunk: false },
    });

    const result = applySubscriptionEvent(state, event);

    expect(result).toMatchObject({ kind: "resyncRequired", reason: "missingChatItem" });
    expect(result.state.cursor).toBe("cursor-1");
  });

  it("replaces a task subscription with task snapshot updates for the same task", () => {
    const state = createSubscriptionIngestionState(subscribeResult(taskScope("task-1"), taskSnapshot("task-1"), "cursor-1"), {
      stateRootId: stateRoot("root-1"),
    });
    const updatedTask = taskSnapshotBody("task-1", [chatItem("message-1", "Done")]);
    const event = taskEvent("root-1", "task-1", "cursor-1", "cursor-2", {
      kind: "taskSnapshotUpdated",
      task: updatedTask,
    });

    const result = applySubscriptionEvent(state, event);

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied") throw new Error("expected applied task snapshot update");
    expect(result.state.snapshot.kind).toBe("task");
    if (result.state.snapshot.kind !== "task") throw new Error("expected task snapshot");
    expect(result.state.snapshot.task).toBe(updatedTask);
  });

  it("uses the history synchronization state from an authoritative task snapshot", () => {
    const snapshot = taskSnapshot("task-1");
    if (snapshot.kind !== "task") throw new Error("expected task snapshot");
    snapshot.task.historySync = { state: "syncing", generation: 4 };
    const state = createSubscriptionIngestionState(subscribeResult(taskScope("task-1"), snapshot, "cursor-1"), {
      stateRootId: stateRoot("root-1"),
    });
    const olderTask = taskSnapshotBody("task-1", [chatItem("message-1", "Replayed")]);
    olderTask.historySync = {
      state: "updated",
      generation: 3,
    };
    const event = taskEvent("root-1", "task-1", "cursor-1", "cursor-2", {
      kind: "taskSnapshotUpdated",
      task: olderTask,
    });

    const result = applySubscriptionEvent(state, event);

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied" || result.state.snapshot.kind !== "task") {
      throw new Error("expected applied task snapshot update");
    }
    expect(result.state.snapshot.task.historySync).toEqual({
      state: "updated",
      generation: 3,
    });
    expect(result.state.snapshot.task.chat.items).toHaveLength(1);
  });

  it("does not revive a superseded history completion from client-side state", () => {
    const snapshot = taskSnapshot("task-1");
    if (snapshot.kind !== "task") throw new Error("expected task snapshot");
    snapshot.task.historySync = { state: "updated", generation: 4 };
    const state = createSubscriptionIngestionState(subscribeResult(taskScope("task-1"), snapshot, "cursor-1"), {
      stateRootId: stateRoot("root-1"),
    });
    const optionMutationSnapshot = taskSnapshotBody("task-1");
    optionMutationSnapshot.revision = 2;
    optionMutationSnapshot.historySync = { state: "syncing", generation: 2 };
    const event = taskEvent("root-1", "task-1", "cursor-1", "cursor-2", {
      kind: "taskSnapshotUpdated",
      task: optionMutationSnapshot,
    });

    const result = applySubscriptionEvent(state, event);

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied" || result.state.snapshot.kind !== "task") {
      throw new Error("expected applied task snapshot update");
    }
    expect(result.state.snapshot.task.historySync).toEqual({ state: "syncing", generation: 2 });
  });

  it("applies task history synchronization updates", () => {
    const state = createSubscriptionIngestionState(
      subscribeResult(taskScope("task-1"), taskSnapshot("task-1"), "cursor-1"),
      { stateRootId: stateRoot("root-1") },
    );
    const event = taskEvent("root-1", "task-1", "cursor-1", "cursor-2", {
      kind: "taskHistorySyncUpdated",
      taskId: taskId("task-1"),
      historySync: { state: "syncing", generation: 2 },
    });

    const result = applySubscriptionEvent(state, event);

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied" || result.state.snapshot.kind !== "task") {
      throw new Error("expected applied task history synchronization update");
    }
    expect(result.state.snapshot.task.historySync).toEqual({ state: "syncing", generation: 2 });
  });

  it("upserts task-scoped pending request updates", () => {
    const existing = pendingRequest("request-1", "task-1", "Old title");
    const snapshot: SubscriptionSnapshot = {
      kind: "task",
      task: { ...taskSnapshotBody("task-1"), pendingRequests: [existing] },
    };
    const state = createSubscriptionIngestionState(subscribeResult(taskScope("task-1"), snapshot, "cursor-1"), {
      stateRootId: stateRoot("root-1"),
    });

    const added = taskEvent("root-1", "task-1", "cursor-1", "cursor-2", {
      kind: "requestUpdated",
      request: pendingRequest("request-2", "task-1", "Approve change"),
    });
    const addResult = applySubscriptionEvent(state, added);
    expect(addResult.kind).toBe("applied");
    if (addResult.kind !== "applied") throw new Error("expected request insertion");
    expect(addResult.state.snapshot.kind).toBe("task");
    if (addResult.state.snapshot.kind !== "task") throw new Error("expected task snapshot");
    expect(addResult.state.snapshot.task.pendingRequests?.map((request) => request.requestId)).toEqual([
      requestId("request-1"),
      requestId("request-2"),
    ]);

    const replaced = taskEvent("root-1", "task-1", "cursor-2", "cursor-3", {
      kind: "requestUpdated",
      request: pendingRequest("request-1", "task-1", "Updated title"),
    });
    const replaceResult = applySubscriptionEvent(addResult.state, replaced);
    expect(replaceResult.kind).toBe("applied");
    if (replaceResult.kind !== "applied") throw new Error("expected request replacement");
    expect(replaceResult.state.snapshot.kind).toBe("task");
    if (replaceResult.state.snapshot.kind !== "task") throw new Error("expected task snapshot");
    expect(replaceResult.state.snapshot.task.pendingRequests?.map((request) => request.title)).toEqual([
      "Updated title",
      "Approve change",
    ]);
  });

  it("applies project collection updates to project subscriptions", () => {
    const state = createSubscriptionIngestionState(
      subscribeResult({ kind: "projects" }, { kind: "projects", projects: { projects: [] } }, "cursor-1"),
      { stateRootId: stateRoot("root-1") },
    );
    const event = projectCollectionEvent("root-1", "cursor-1", "cursor-2");

    const result = applySubscriptionEvent(state, event);

    expect(result.kind).toBe("applied");
    if (result.kind !== "applied") throw new Error("expected project update");
    expect(result.snapshotChanged).toBe(true);
    expect(result.state.cursor).toBe("cursor-2");
    expect(result.state.snapshot.kind).toBe("projects");
    if (result.state.snapshot.kind !== "projects") throw new Error("expected projects snapshot");
    expect(result.state.snapshot.projects.projects[0]?.projectId).toBe(projectId("project-1"));
  });

  it("resyncs replacement state-root events when the client stream skipped cursor steps", () => {
    const state = createSubscriptionIngestionState(
      subscribeResult({ kind: "projects" }, { kind: "projects", projects: { projects: [] } }, "cursor-504"),
      { stateRootId: stateRoot("root-1") },
    );
    const event = projectCollectionEvent("root-1", "cursor-506", "cursor-507");

    const result = applySubscriptionEvent(state, event);

    expect(result).toMatchObject({ kind: "resyncRequired", reason: "cursorGap" });
    expect(result.state.cursor).toBe("cursor-504");
  });

  it("ignores duplicate replacement state-root events with an already-applied cursor", () => {
    const state = createSubscriptionIngestionState(
      subscribeResult({ kind: "projects" }, { kind: "projects", projects: { projects: [{ projectId: projectId("project-1"), label: "Project" }] } }, "cursor-507"),
      { stateRootId: stateRoot("root-1") },
    );
    const duplicate = projectCollectionEvent("root-1", "cursor-506", "cursor-507");

    const result = applySubscriptionEvent(state, duplicate);

    expect(result).toMatchObject({ kind: "ignored", reason: "subscriptionMismatch" });
    expect(result.state.cursor).toBe("cursor-507");
  });

  it("advances task navigation cursors for project collection updates without changing navigation", () => {
    const snapshot: SubscriptionSnapshot = {
      kind: "taskNavigation",
      navigation: taskNavigation([taskSummary("task-1")]),
    };
    const state = createSubscriptionIngestionState(subscribeResult({ kind: "taskNavigation" }, snapshot, "cursor-1"), {
      stateRootId: stateRoot("root-1"),
    });
    const projectEvent = projectCollectionEvent("root-1", "cursor-1", "cursor-2");
    const taskNavigationEvent: AppServerEvent = {
      previousCursor: eventCursor("cursor-2"),
      cursor: eventCursor("cursor-3"),
      scope: { kind: "stateRoot", stateRootId: stateRoot("root-1") },
      payload: { kind: "taskNavigationUpdated", navigation: taskNavigation([taskSummary("task-1"), taskSummary("task-2")]) },
    };

    const projectResult = applySubscriptionEvent(state, projectEvent);
    expect(projectResult.kind).toBe("applied");
    if (projectResult.kind !== "applied") throw new Error("expected project cursor advancement");
    expect(projectResult.snapshotChanged).toBe(false);
    expect(projectResult.state.cursor).toBe("cursor-2");

    const navigationResult = applySubscriptionEvent(projectResult.state, taskNavigationEvent);
    expect(navigationResult.kind).toBe("applied");
    if (navigationResult.kind !== "applied") throw new Error("expected navigation update after project event");
    expect(navigationResult.state.cursor).toBe("cursor-3");
    expect(navigationResult.state.snapshot.kind).toBe("taskNavigation");
    if (navigationResult.state.snapshot.kind !== "taskNavigation") throw new Error("expected task navigation snapshot");
    expect(navigationResult.state.snapshot.navigation.tasks.map((task) => task.taskId)).toEqual([
      taskId("task-1"),
      taskId("task-2"),
    ]);
  });

  it("compares optional subscription scope fields consistently", () => {
    expect(subscriptionScopesEqual({ kind: "settings" }, { kind: "settings", section: null })).toBe(true);
    expect(subscriptionScopesEqual({ kind: "taskNavigation" }, { kind: "taskNavigation", projectId: null })).toBe(true);
    expect(
      subscriptionScopesEqual(
        { kind: "taskNavigation", projectId: projectId("project-1") },
        { kind: "taskNavigation", projectId: projectId("project-2") },
      ),
    ).toBe(false);
  });
});

function subscribeResult(scope: SubscriptionScope, snapshot: SubscriptionSnapshot, cursor: string): StateSubscribeResult {
  return { scope, snapshot, cursor: eventCursor(cursor) };
}

function taskScope(id: string): SubscriptionScope {
  return { kind: "task", taskId: taskId(id) };
}

function taskSnapshot(id: string, items: ChatItem[] = []): SubscriptionSnapshot {
  return {
    kind: "task",
    task: taskSnapshotBody(id, items),
  };
}

function taskSnapshotBody(id: string, items: ChatItem[] = []): TaskSnapshot {
  return {
    task: taskSummary(id),
    lifecycle: "visible",
    revision: 1,
    preparation: { kind: "ready" as const },
    agentConfig: { state: "ready" as const, options: [] },
    agentCommands: { state: "ready" as const, commands: [] },
    sendCapability: { state: "ready" as const },
    historySync: { state: "idle" as const, generation: 0 },
    chat: { items, hasMessages: items.length > 0 },
    pendingRequests: [],
  };
}

function taskSummary(id: string, project = "project-1"): TaskSummary {
  return {
    taskId: taskId(id),
    projectId: projectId(project),
    agentId: agentId("agent-1"),
    title: { value: `Task ${id}`, source: "user" },
    status: "idle",
    updatedAt: "2026-06-26T00:00:00.000Z",
    lastActivity: "2026-06-26T00:00:00.000Z",
    unread: false,
    hasMessages: true,
  };
}

function taskNavigation(tasks: TaskSummary[], activeTaskId: string | null = null): TaskNavigationSnapshot {
  return { tasks, activeTaskId: activeTaskId ? taskId(activeTaskId) : null };
}

function chatItem(id: string, text: string): ChatItem {
  return {
    messageId: messageId(id),
    role: "agent",
    status: "streaming",
    parts: [{ kind: "text", text }],
  };
}

function pendingRequest(id: string, task: string, title: string): PendingRequestSnapshot {
  return {
    requestId: requestId(id),
    scope: { kind: "task", taskId: taskId(task) },
    kind: "permission",
    title,
  };
}

function taskEvent(
  rootId: string,
  id: string,
  previousCursor: string,
  cursor: string,
  payload: AppServerEvent["payload"],
): AppServerEvent {
  return {
    previousCursor: eventCursor(previousCursor),
    cursor: eventCursor(cursor),
    scope: { kind: "task", stateRootId: stateRoot(rootId), taskId: taskId(id) },
    payload,
  };
}

function projectCollectionEvent(rootId: string, previousCursor: string, cursor: string): AppServerEvent {
  return {
    previousCursor: eventCursor(previousCursor),
    cursor: eventCursor(cursor),
    scope: { kind: "stateRoot", stateRootId: stateRoot(rootId) },
    payload: {
      kind: "projectCollectionUpdated",
      projects: {
        projects: [{ projectId: projectId("project-1"), label: "Project" }],
      },
    },
  };
}

function clientSnapshot(
  rootId: string,
  clientId: string,
  snapshot: Partial<Pick<ClientSnapshot, "tasks">> = {},
): ClientSnapshot {
  return {
    cursor: eventCursor("cursor-1"),
    server: { serverId: serverId("server-1"), protocolVersion: { major: 1, minor: 0 } },
    stateRoot: { stateRootId: stateRoot(rootId) },
    client: { clientInstanceId: clientInstance(clientId), shellKind: "web", surface: { kind: "home" } },
    newTaskDefaults: {},
    pendingRequests: [],
    ...snapshot,
  };
}

function eventCursor(value: string): EventCursor {
  return value as EventCursor;
}

function stateRoot(value: string): StateRootId {
  return value as StateRootId;
}

function serverId(value: string): ServerId {
  return value as ServerId;
}

function clientInstance(value: string): ClientInstanceId {
  return value as ClientInstanceId;
}

function taskId(value: string): TaskId {
  return value as TaskId;
}

function projectId(value: string): ProjectId {
  return value as ProjectId;
}

function messageId(value: string): MessageId {
  return value as MessageId;
}

function requestId(value: string): RequestId {
  return value as RequestId;
}

function agentId(value: string): AgentId {
  return value as AgentId;
}
