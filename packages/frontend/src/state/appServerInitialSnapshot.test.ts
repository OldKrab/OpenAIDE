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
  it("maps initialization state without creating a Navigation replica", () => {
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
      { type: "newTask:agent", agentId: "codex" },
      { type: "settings:runtimeSettings", settings: { developer: { acp_trace: { enabled: true } } } },
      { type: "settings:preferences", preferences: { composer_submit_shortcut: "enter" } },
      { type: "snapshot", intent: "open", snapshot: { task: { task_id: "task-1" } } },
    ]);
    expect(ingestion.requiresNativeSurface).toBe(false);
  });

  it("returns no actions when initialize did not include renderable task state", () => {
    expect(actionsFromInitialSnapshot(clientSnapshot({ projects: null, tasks: null, activeTask: null }))).toEqual({
      actions: [{ type: "newTask:agent", agentId: "codex", agentLabel: "Codex" }],
      warnings: [],
      requiresNativeSurface: false,
    });
  });

  it("can suppress the focused Task while Navigation remains subscription-owned", () => {
    expect(actionsFromInitialSnapshot(clientSnapshot(), { includeActiveTask: false }).actions).toMatchObject([
      { type: "projects" },
      { type: "newTask:agent" },
    ]);
  });

  it("uses retained choices before shell and App Server defaults", () => {
    const ingestion = actionsFromInitialSnapshot(clientSnapshot({
      client: {
        clientInstanceId: "client-1" as ClientInstanceId,
        shellKind: "web",
        surface: { kind: "newTask", projectId: "project-2" as ProjectId },
      },
      projects: {
        projects: [
          { projectId: "project-1" as ProjectId, label: "API", workspaceRoot: "/workspace/API", available: true },
          { projectId: "project-2" as ProjectId, label: "App", workspaceRoot: "/workspace/App", available: true },
        ],
      },
      agents: {
        agents: [
          { agentId: "codex" as AgentId, label: "Codex", status: "connected" },
          { agentId: "opencode" as AgentId, label: "OpenCode", status: "connected" },
        ],
      },
      newTaskDefaults: {
        projectId: "project-1" as ProjectId,
        agentId: "codex" as AgentId,
      },
      tasks: null,
      activeTask: null,
    }), {
      retainedNewTaskContext: { projectId: "project-2", agentId: "opencode" },
    });

    expect(ingestion.actions[0]).toEqual({
      type: "projects",
      initialProjectId: "project-2",
      projects: [
        {
          projectId: "project-1",
          label: "API",
          workspaceRoot: "/workspace/API",
          available: true,
          projectWorktreeId: undefined,
          worktreeError: undefined,
          worktreeRepositoryId: undefined,
        },
        {
          projectId: "project-2",
          label: "App",
          workspaceRoot: "/workspace/App",
          available: true,
          projectWorktreeId: undefined,
          worktreeError: undefined,
          worktreeRepositoryId: undefined,
        },
      ],
    });
    expect(ingestion.actions[1]).toEqual({
      type: "newTask:agent",
      agentId: "opencode",
      agentLabel: "OpenCode",
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
    newTaskDefaults: {},
    projects: {
      projects: [{ projectId: "project-1" as ProjectId, label: "Project", workspaceRoot: "/workspace/Project", available: true }],
    },
    agents: {
      agents: [{ agentId: "codex" as AgentId, label: "Codex", status: "connected" }],
    },
    tasks: {
      section: "tasks",
      refresh: { state: "idle" },
      groups: [{
        projectId: "project-1" as ProjectId,
        projectLabel: "Project",
        taskCount: 1,
        entries: [
        { kind: "task", task: taskSummary() },
        {
          kind: "nativeSession",
          session: {
            reference: { agentId: "codex" as AgentId, sessionId: "native-1" },
            projectId: "project-1" as ProjectId,
            workspaceRoot: "/workspace/Project",
            title: "Cached Native Session",
          },
        },
        ],
      }],
    },
    activeTask: {
      task: taskSummary(),
      lifecycle: "open",
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
    lifecycle: "open" as const,
    title: { value: "Task", source: "user" as const },
    status: "idle" as const,
    updatedAt: "2026-06-27T12:00:00.000Z",
    lastActivity: "2026-06-27T12:00:00.000Z",
    unread: false,
    hasMessages: true,
    workspaceAvailable: true,
  };
}
