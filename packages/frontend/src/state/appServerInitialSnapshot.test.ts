import { describe, expect, it } from "vitest";
import type {
  AgentId,
  ClientInstanceId,
  ClientSnapshot,
  EventCursor,
  ProjectId,
  ServerId,
  StateRootId,
  TaskId,
} from "@openaide/app-server-client";
import { actionsFromInitialSnapshot } from "./appServerInitialSnapshot";

describe("App Server initial snapshot ingestion", () => {
  it("maps initial task navigation and active task into current app actions", () => {
    const ingestion = actionsFromInitialSnapshot(clientSnapshot({
      settings: {
        sections: [],
        runtime: {
          developer: {
            acpTrace: { enabled: true, directory: "/runtime/traces" },
          },
        },
        preferences: {
          preferences: { composerSubmitShortcut: "enter" },
        },
      },
    }));

    expect(ingestion.actions).toMatchObject([
      { type: "projects", projects: [{ projectId: "project-1", label: "Project" }] },
      { type: "settings:runtimeSettings", settings: { developer: { acp_trace: { enabled: true } } } },
      { type: "settings:preferences", preferences: { composer_submit_shortcut: "enter" } },
      {
        type: "tasks",
        archived: false,
        tasks: [{ task_id: "task-1", agent_name: "Codex", workspace_root: "" }],
      },
      { type: "selection:set", taskId: "task-1" },
      { type: "snapshot", intent: "open", snapshot: { task: { task_id: "task-1" } } },
    ]);
    expect(ingestion.requiresNativeSurface).toBe(false);
  });

  it("returns no actions when initialize did not include renderable task state", () => {
    expect(actionsFromInitialSnapshot(clientSnapshot({ projects: null, tasks: null, activeTask: null }))).toEqual({
      actions: [],
      warnings: [],
      requiresNativeSurface: false,
    });
  });

  it("can suppress task slices that were already loaded by legacy startup", () => {
    expect(actionsFromInitialSnapshot(clientSnapshot(), { includeTaskNavigation: false }).actions).toMatchObject([
      { type: "projects" },
      { type: "snapshot" },
    ]);
    expect(actionsFromInitialSnapshot(clientSnapshot(), { includeActiveTask: false }).actions).toMatchObject([
      { type: "projects" },
      { type: "tasks" },
      { type: "selection:set" },
    ]);
  });

  it("uses a requested new-task project before the active project fallback", () => {
    const ingestion = actionsFromInitialSnapshot(clientSnapshot({
      client: {
        clientInstanceId: "client-1" as ClientInstanceId,
        shellKind: "web",
        surface: { kind: "newTask", projectId: "project-2" as ProjectId },
      },
      projects: {
        activeProjectId: "project-1" as ProjectId,
        projects: [
          { projectId: "project-1" as ProjectId, label: "API" },
          { projectId: "project-2" as ProjectId, label: "App" },
        ],
      },
      tasks: null,
      activeTask: null,
    }));

    expect(ingestion.actions[0]).toEqual({
      type: "projects",
      activeProjectId: "project-2",
      projects: [
        { projectId: "project-1", label: "API" },
        { projectId: "project-2", label: "App" },
      ],
    });
  });
});

function clientSnapshot(overrides: Partial<ClientSnapshot> = {}): ClientSnapshot {
  return {
    cursor: "cursor-1" as EventCursor,
    server: {
      serverId: "server-1" as ServerId,
      protocolVersion: { major: 1, minor: 0 },
    },
    stateRoot: { stateRootId: "state-root-1" as StateRootId },
    client: {
      clientInstanceId: "client-1" as ClientInstanceId,
      shellKind: "vscodeExtension",
      surface: { kind: "task", taskId: "task-1" as TaskId },
    },
    projects: {
      projects: [{ projectId: "project-1" as ProjectId, label: "Project" }],
    },
    agents: {
      agents: [{ agentId: "codex" as AgentId, label: "Codex", status: "connected" }],
    },
    tasks: {
      activeTaskId: "task-1" as TaskId,
      tasks: [taskSummary()],
    },
    activeTask: {
      task: taskSummary(),
      revision: 2,
      preparation: { kind: "ready" },
      agentConfig: { state: "ready", options: [] },
      agentCommands: { state: "ready", commands: [] },
      sendCapability: { state: "ready" },
      historySync: { state: "idle", generation: 0 },
      chat: { items: [], hasMoreBefore: false, hasMessages: true },
    },
    ...overrides,
  };
}

function taskSummary() {
  return {
    taskId: "task-1" as TaskId,
    projectId: "project-1" as ProjectId,
    agentId: "codex" as AgentId,
    title: "Task",
    status: "idle" as const,
    updatedAt: "2026-06-27T12:00:00.000Z",
    lastActivity: "2026-06-27T12:00:00.000Z",
    unread: false,
    hasMessages: true,
  };
}
