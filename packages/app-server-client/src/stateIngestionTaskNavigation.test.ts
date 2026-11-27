import { describe, expect, it } from "vitest";
import type {
  AppServerEvent,
  NavigationFrame,
  NavigationGroupFrame,
  NavigationRow,
  ProjectId,
  SubscriptionScope,
  TaskId,
  TaskSummary,
} from "./generated/protocol.js";
import { applySubscriptionEvent, installNavigationSubscribeReply } from "./stateIngestion.js";
import {
  applyNavigationFrame,
  installNavigationReply,
  navigationGroupRows,
} from "./stateIngestionTaskNavigation.js";

const scope: SubscriptionScope = { kind: "taskNavigation", section: "tasks" };
const one = "project-1" as ProjectId;
const two = "project-2" as ProjectId;

describe("Navigation frame ingestion", () => {
  it("installs the baseline in the server's group and row order", () => {
    const navigation = installNavigationReply({
      frame: frame({
        groupOrder: [two, one],
        groups: [group(one, ["b", "a"]), group(two, ["c"])],
      }),
    });
    expect(navigation?.groups.map((entry) => entry.projectId)).toEqual([two, one]);
    expect(navigation?.groups[1] && navigationGroupRows(navigation.groups[1]).map(rowTaskId)).toEqual(["b", "a"]);
  });

  it("carries unchanged rows over and replaces changed ones by the frame's order", () => {
    const base = installNavigationReply({
      frame: frame({ groupOrder: [one], groups: [group(one, ["a", "b"])] }),
    })!;
    const next = applyNavigationFrame(base, frame({ revision: 2,
      groups: [{ ...group(one, ["b", "a"]), rows: { "task:a": row("a", { unread: true }) } }],
    }));
    expect(next.kind).toBe("applied");
    if (next.kind !== "applied") return;
    const rows = navigationGroupRows(next.navigation.groups[0]!);
    expect(rows.map(rowTaskId)).toEqual(["b", "a"]);
    expect(rows[1]?.entry.kind === "task" && rows[1].entry.task.unread).toBe(true);
    expect(rows[0]).toBe(base.groups[0]!.rowsById["task:b"]);
  });

  it("leaves groups the frame does not mention untouched and drops removed groups", () => {
    const base = installNavigationReply({
      frame: frame({ groupOrder: [one, two], groups: [group(one, ["a"]), group(two, ["b"])] }),
    })!;
    const next = applyNavigationFrame(base, frame({ revision: 2, groupOrder: [one], removedGroups: [two] }));
    expect(next.kind === "applied" && next.navigation.groups.map((entry) => entry.projectId)).toEqual([one]);
    const untouched = applyNavigationFrame(base, frame({ revision: 2, groups: [] }));
    expect(untouched.kind === "applied" && untouched.navigation.groups[0]).toBe(base.groups[0]);
  });

  it("carries counts, discovery and undiscovered state from the frame", () => {
    const next = installNavigationReply({
      frame: frame({
        groupOrder: [one],
        groups: [{ ...group(one, ["a"]), total: 9, hidden: 8, undiscovered: true, discovery: "failed" }],
      }),
    });
    expect(next?.groups[0]).toMatchObject({ total: 9, hidden: 8, undiscovered: true, discovery: "failed" });
  });

  it("asks for a resync when a frame omits a row the client never held", () => {
    const base = installNavigationReply({
      frame: frame({ groupOrder: [one], groups: [group(one, ["a"])] }),
    })!;
    expect(applyNavigationFrame(base, frame({ revision: 2,
      groups: [{ ...group(one, ["a", "lost"]), rows: {} }],
    }))).toEqual({ kind: "resyncRequired", reason: "missingNavigationRow" });
    expect(applyNavigationFrame(base, frame({ revision: 2, groupOrder: [one, two] })))
      .toEqual({ kind: "resyncRequired", reason: "missingNavigationGroup" });
  });

  it("ingests the frames of its subscription", () => {
    const state = installNavigationSubscribeReply({
      frame: frame({ groupOrder: [one], groups: [group(one, ["a"])] }),
    }, scope)!;
    const applied = applySubscriptionEvent(state, event(frame({
      revision: 2,
      groups: [{ ...group(one, ["a", "b"]), total: 2 }],
    })));
    expect(applied.kind).toBe("applied");
    if (applied.kind !== "applied" || applied.state.snapshot.kind !== "taskNavigation") return;
    expect(applied.state.snapshot.navigation.groups[0]?.order).toEqual(["task:a", "task:b"]);

  });

  it("drops a frame sent before the baseline it would overtake", () => {
    const state = installNavigationSubscribeReply({
      frame: frame({ revision: 5, groupOrder: [one], groups: [group(one, ["a"])] }),
    }, scope)!;

    const stale = applySubscriptionEvent(state, event(frame({
      revision: 4,
      groups: [{ ...group(one, ["z"]), total: 1 }],
    })));

    expect(stale.kind).toBe("applied");
    if (stale.kind !== "applied") return;
    expect(stale.snapshotChanged).toBe(false);
    expect(stale.state.snapshot).toBe(state.snapshot);
  });
});

function frame(overrides: Partial<NavigationFrame>): NavigationFrame {
  return { section: "tasks", revision: 1, removedGroups: [], groups: [], ...overrides };
}

function group(projectId: ProjectId, ids: string[]): NavigationGroupFrame {
  return {
    projectId,
    projectLabel: String(projectId),
    order: ids.map((id) => `task:${id}`),
    rows: Object.fromEntries(ids.map((id) => [`task:${id}`, row(id)])),
    total: ids.length,
    hidden: 0,
    undiscovered: false,
    discovery: "settled",
  };
}

function row(id: string, overrides: Partial<TaskSummary> = {}): NavigationRow {
  return { live: false, entry: { kind: "task", task: { ...summary(id), ...overrides } } };
}

function rowTaskId(value: NavigationRow) {
  return value.entry.kind === "task" ? value.entry.task.taskId : undefined;
}

function event(navigation: NavigationFrame): AppServerEvent {
  return {
    subscription: scope,
    payload: { kind: "navigationFrame", frame: navigation },
  } as AppServerEvent;
}

function summary(id: string): TaskSummary {
  return {
    taskId: id as TaskId,
    projectId: one,
    agentId: "agent-1" as never,
    lifecycle: "open",
    title: { value: "Task", source: "user" },
    status: "idle",
    updatedAt: "1",
    lastActivity: "1",
    unread: false,
    hasMessages: true,
    workspaceAvailable: true,
  };
}

