import { describe, expect, it } from "vitest";
import type {
  AgentId,
  ClientInstanceId,
  MessageId,
  ProjectId,
  RequestId,
  TaskSummary as ProtocolTaskSummary,
  TaskId,
  TaskSnapshot as ProtocolTaskSnapshot,
} from "@openaide/app-server-client";
import {
  createProtocolTaskSnapshotMapper,
  mapProtocolConfigOptions,
  mapProtocolAgentCommands,
  mapProtocolTaskNavigation,
  mapProtocolTaskSnapshot,
  mapProtocolTaskSummary,
} from "./appServerProtocolMapping";
import { renderedChat } from "./chatPaging";

describe("App Server Protocol state mapping", () => {
  it("maps task navigation summaries into current frontend task summaries", () => {
    expect(mapProtocolTaskNavigation({
      activeTaskId: "task-1" as TaskId,
      tasks: [
        protocolSummary({ taskId: "task-1" as TaskId, status: "running" }),
        protocolSummary({ taskId: "task-2" as TaskId, status: "waiting", unread: true }),
      ],
    }, mappingContext())).toMatchObject({
      activeTaskId: "task-1",
      tasks: [
        { task_id: "task-1", status: "active", agent_id: "codex", agent_name: "Codex", workspace_root: "" },
        { task_id: "task-2", status: "waiting", unread: true },
      ],
      warnings: [],
      requiresNativeSurface: false,
    });
  });

  it("keeps stopping distinct from running", () => {
    expect(mapProtocolTaskSummary(protocolSummary({ status: "stopping" }))).toMatchObject({
      status: "stopping",
    });
  });

  it("maps task list activity separately from the persistence update timestamp", () => {
    expect(mapProtocolTaskSummary(protocolSummary({
      updatedAt: "2026-06-27T12:00:05.000Z",
      lastActivity: "2026-06-27T12:00:00.000Z",
    }))).toMatchObject({
      updated_at: "2026-06-27T12:00:05.000Z",
      last_activity: "2026-06-27T12:00:00.000Z",
    });
  });

  it("maps explicit Task Attention without inferring it from status or unread", () => {
    expect(mapProtocolTaskSummary(protocolSummary({
      status: "waiting",
      unread: true,
      attention: {
        eventId: "attention-1",
        reason: "needsPermission",
        occurredAt: "2026-06-27T12:00:04.000Z",
      },
    }))).toMatchObject({
      attention: {
        event_id: "attention-1",
        reason: "needsPermission",
        occurred_at: "2026-06-27T12:00:04.000Z",
      },
    });
    expect(mapProtocolTaskSummary(protocolSummary({ status: "waiting", unread: true })).attention).toBeUndefined();
  });

  it("falls back to the update timestamp when task navigation omits last activity", () => {
    const summary: Partial<ProtocolTaskSummary> = protocolSummary({
      updatedAt: "2026-06-27T12:00:05.000Z",
    });
    delete summary.lastActivity;

    expect(mapProtocolTaskSummary(summary as ProtocolTaskSummary)).toMatchObject({
      updated_at: "2026-06-27T12:00:05.000Z",
      last_activity: "2026-06-27T12:00:05.000Z",
    });
  });

  it("omits prepared no-message tasks from sidebar navigation", () => {
    const mapping = mapProtocolTaskNavigation({
      activeTaskId: "task-sent" as TaskId,
      tasks: [
        protocolSummary({
          taskId: "task-empty" as TaskId,
          title: { value: "New task", source: "user" },
          status: "idle",
          hasMessages: false,
        }),
        protocolSummary({
          taskId: "task-sent" as TaskId,
          title: { value: "Implement feature", source: "user" },
          status: "running",
          hasMessages: true,
        }),
      ],
    }, mappingContext());

    expect(mapping.tasks.map((task) => task.task_id)).toEqual(["task-sent"]);
    expect(mapping.activeTaskId).toBe("task-sent");
  });

  it("uses lifecycle-specific presentation titles when App Server has no title", () => {
    expect(mapProtocolTaskSummary(protocolSummary({ title: null })).title).toBe("Untitled task");
    const newTask = mapProtocolTaskSnapshot(protocolSnapshot({
      lifecycle: "new",
      task: protocolSummary({ title: null }),
    })).snapshot;
    expect(newTask.task.title).toBe("New task");
    expect(newTask.lifecycle).toBe("new");
  });

  it("maps task snapshots into current frontend task snapshots", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot(), mappingContext());
    const snapshot = mapping.snapshot;

    expect(snapshot.task).toMatchObject({
      task_id: "task-1",
      status: "active",
      agent_name: "Codex",
      workspace_root: "",
      task_version: 7,
      message_history_version: 7,
      has_messages: true,
    });
    expect(snapshot.chat).toMatchObject({
      task_id: "task-1",
      version: 7,
      has_before: true,
      has_messages: true,
      total_count: 3,
      start_cursor: "m:1",
      end_cursor: "m:3",
    });
    expect(snapshot.chat.items.map((item) => item.message)).toMatchObject([
      {
        kind: "user",
        text: "hello",
        created_at: "2026-06-27T12:00:00.000Z",
        attachments: [{ kind: "file", label: "README.md" }],
      },
      {
        kind: "agent_message",
        role: "agent",
        parts: [{ kind: "text", text: "world" }],
        created_at: "2026-06-27T12:00:00.000Z",
      },
      {
        kind: "activity",
        title: "Running tests",
        created_at: "2026-06-27T12:00:00.000Z",
        status: "error",
        collapsed: true,
        steps: [
          { kind: "command", command_label: "npm test", status: "error", exit_code: 1, output_preview: "failed" },
          {
            kind: "tool",
            tool_call_id: "tool-1",
            name: "edit",
            status: "completed",
            input_summary: "Editing files",
            detail_artifact_id: "artifact_1",
          },
        ],
      },
    ]);
    expect(snapshot.settings_summary.config_options).toEqual({ model: "gpt-5" });
    expect(snapshot.send_capability).toEqual({ state: "ready" });
    expect(mapping.warnings).toEqual([]);
    expect(mapping.requiresNativeSurface).toBe(false);
  });

  it("preserves unchanged Chat row identity across focused text updates", () => {
    const mapSnapshot = createProtocolTaskSnapshotMapper();
    const initial = protocolSnapshot();
    const first = mapSnapshot(initial, mappingContext()).snapshot;
    const updatedAgent = {
      ...initial.chat.items[1]!,
      parts: [{ kind: "text" as const, text: "world continued" }],
    };
    const second = mapSnapshot({
      ...initial,
      revision: initial.revision + 1,
      chat: {
        ...initial.chat,
        items: [initial.chat.items[0]!, updatedAgent, initial.chat.items[2]!],
      },
    }, mappingContext()).snapshot;

    expect(second.chat.items[0]).toBe(first.chat.items[0]);
    expect(second.chat.items[1]).not.toBe(first.chat.items[1]);
    expect(second.chat.items[2]).toBe(first.chat.items[2]);
  });

  it("keeps the ACP tool title as the visible fallback when tool input is absent", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot({
      chat: {
        hasMoreBefore: false,
        hasMessages: true,
        items: [
          {
            messageId: "search-1" as MessageId,
            role: "agent",
            status: "complete",
            parts: [
              {
                kind: "activity",
                title: "Search for 'activityLabels' in frontend",
                status: "completed",
                steps: [{ kind: "tool", name: "search", status: "completed", permissionOutcomes: [] }],
              },
            ],
          },
        ],
      },
    }), mappingContext());

    expect(mapping.snapshot.chat.items[0].message).toMatchObject({
      kind: "activity",
      steps: [
        {
          kind: "tool",
          name: "search",
          input_summary: "Search for 'activityLabels' in frontend",
        },
      ],
    });
  });

  it("warns when a task references a project missing from the project collection", () => {
    const mapping = mapProtocolTaskNavigation({
      activeTaskId: "task-1" as TaskId,
      tasks: [protocolSummary({ taskId: "task-1" as TaskId })],
    }, { agents: mappingContext().agents, projects: [] });

    expect(mapping.warnings).toEqual([{ kind: "projectDisplayNotMapped", projectId: "project-1" }]);
  });

  it("uses known built-in Agent labels when the Agent collection is not available yet", () => {
    expect(mapProtocolTaskSummary(protocolSummary({ agentId: "codex" as AgentId })).agent_name).toBe("Codex");
    expect(mapProtocolTaskSummary(protocolSummary({ agentId: "opencode" as AgentId })).agent_name).toBe("OpenCode");
  });

  it("maps protocol image attachment previews into chat attachment payloads", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot({
      chat: {
        hasMoreBefore: false,
        hasMessages: true,
        items: [
          {
            messageId: "user-image" as MessageId,
            role: "user",
            status: "complete",
            parts: [
              { kind: "text", text: "look" },
              {
                kind: "attachment",
                attachment: {
                  attachmentId: "attachment-image" as never,
                  kind: "embeddedSnapshot",
                  label: "diagram.png",
                  mediaType: "image/png",
                  sizeBytes: 5,
                  previewUrl: "data:image/png;base64,aW1hZ2U=",
                },
              },
            ],
          },
        ],
      },
    }));

    expect(mapping.snapshot.chat.items[0].message).toMatchObject({
      kind: "user",
      attachments: [
        {
          id: "attachment-image",
          kind: "file",
          label: "diagram.png",
          payload: {
            previewUrl: "data:image/png;base64,aW1hZ2U=",
            mimeType: "image/png",
            sizeBytes: 5,
          },
        },
      ],
    });
  });

  it("maps typed Agent content without exposing protocol objects", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot({
      chat: {
        hasMoreBefore: false,
        hasMessages: true,
        items: [
          {
            messageId: "agent-image" as MessageId,
            role: "agent",
            status: "complete",
            parts: [{
              kind: "image",
              mediaType: "image/png",
              dataUrl: "data:image/png;base64,aW1hZ2U=",
              uri: "memory://diagram.png",
            }],
          },
          {
            messageId: "agent-resource" as MessageId,
            role: "agent",
            status: "complete",
            parts: [{
              kind: "resource",
              uri: "memory://notes.txt",
              mediaType: "text/plain",
              text: "Embedded notes",
            }],
          },
          {
            messageId: "agent-audio" as MessageId,
            role: "agent",
            status: "complete",
            parts: [{ kind: "unsupported", contentType: "audio", mediaType: "audio/wav" }],
          },
        ],
      },
    }));

    expect(mapping.snapshot.chat.items.map((item) => item.message)).toMatchObject([
      {
        kind: "agent_message",
        role: "agent",
        parts: [{
          kind: "image",
          media_type: "image/png",
          data_url: "data:image/png;base64,aW1hZ2U=",
          uri: "memory://diagram.png",
        }],
      },
      {
        kind: "agent_message",
        role: "agent",
        parts: [{
          kind: "resource",
          uri: "memory://notes.txt",
          media_type: "text/plain",
          text: "Embedded notes",
        }],
      },
      {
        kind: "agent_message",
        role: "agent",
        parts: [{ kind: "unsupported", content_type: "audio", media_type: "audio/wav" }],
      },
    ]);
  });

  it("keeps every ordered part of one mixed Agent message", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot({
      chat: {
        hasMoreBefore: false,
        hasMessages: true,
        items: [{
          messageId: "agent-mixed" as MessageId,
          role: "agent",
          status: "complete",
          parts: [
            { kind: "text", text: "Before" },
            { kind: "resource", uri: "memory://result.txt", text: "Result" },
            { kind: "text", text: "After" },
            { kind: "unsupported", contentType: "audio", mediaType: "audio/wav" },
          ],
        }],
      },
    }));

    expect(mapping.snapshot.chat.items[0]?.message).toEqual({
      kind: "agent_message",
      id: "agent-mixed",
      role: "agent",
      parts: [
        { kind: "text", text: "Before" },
        {
          kind: "resource",
          uri: "memory://result.txt",
          name: undefined,
          title: undefined,
          description: undefined,
          media_type: undefined,
          size_bytes: undefined,
          text: "Result",
        },
        { kind: "text", text: "After" },
        { kind: "unsupported", content_type: "audio", media_type: "audio/wav", uri: undefined },
      ],
      created_at: "2026-06-27T12:00:00.000Z",
    });
  });

  it("maps task status and config options conservatively", () => {
    expect(mapProtocolTaskSummary(protocolSummary({ status: "idle" }))).toMatchObject({ status: "inactive" });
    expect(mapProtocolTaskSummary(protocolSummary({ status: "interrupted" }))).toMatchObject({ status: "failed" });
    expect(mapProtocolConfigOptions(protocolSnapshot().agentConfig, "codex")).toMatchObject({
      agent_id: "codex",
      status: "ready",
      options: [{ id: "model", category: "model", current_value: "gpt-5", values: [{ id: "gpt-5" }] }],
    });
  });

  it("preserves unavailable and pending Configuration Option state", () => {
    expect(mapProtocolConfigOptions({ state: "unavailable" }, "codex")).toEqual({
      agent_id: "codex",
      status: "unavailable",
      options: [],
    });
    expect(mapProtocolConfigOptions({
      ...protocolSnapshot().agentConfig,
      state: "stale",
      pendingChange: {
        clientMutationId: "mutation-1" as never,
        configId: "model" as never,
        requestedValue: "gpt-5.1",
      },
    }, "codex")).toMatchObject({
      status: "stale",
      pending_change: {
        mutation_id: "mutation-1",
        option_id: "model",
        requested_value: "gpt-5.1",
      },
    });
    expect(mapProtocolAgentCommands({ state: "unavailable" }, "codex")).toBeUndefined();
  });

  it("keeps normal task preparation out of chat while preserving startup state", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot({
      task: protocolSummary({ status: "idle" }),
      chat: { hasMoreBefore: false, hasMessages: false, items: [] },
      preparation: {
        kind: "preparing",
        steps: [{ kind: "creatingNativeSession", label: "Creating Agent session", status: "running" }],
      },
      sendCapability: {
        state: "loading",
        blockers: [{ kind: "taskPreparing", message: "Task Agent preparation is still running" }],
      },
    }));

    expect(mapping.snapshot.chat.items.map((item) => item.message.kind)).not.toContain("permission");
    expect(mapping.snapshot.task.status).toBe("active");
    expect(mapping.snapshot.send_capability).toEqual({
      state: "loading",
      blockers: [{ kind: "taskPreparing", message: "Task Agent preparation is still running" }],
    });
  });

  it("keeps lossy App Server-only state visible and reported", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot({
      preparation: { kind: "blocked", blocker: { kind: "authRequired", message: "Sign in" }, actions: ["authenticate"] },
      agentCommands: { state: "loading" },
      sendCapability: { state: "blocked", blockers: [{ kind: "taskPreparing", message: "Still preparing" }] },
      recovery: { message: "Task detached", actions: ["continue"] },
      pendingRequests: [
        {
          requestId: "request-1" as RequestId,
          kind: "permission",
          title: "Allow command?",
          scope: { kind: "task", taskId: "task-1" as TaskId },
        },
        {
          requestId: "client-request-1" as RequestId,
          kind: "secret",
          title: "Secret",
          scope: { kind: "client", clientInstanceId: "client-1" as ClientInstanceId },
        },
      ],
    }));

    expect(mapping.snapshot.task.status).toBe("waiting");
    expect(mapping.snapshot.chat.items.map((item) => item.message)).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          kind: "interruption",
          message: "Sign in",
          recoverable: true,
        }),
        expect.objectContaining({
          kind: "interruption",
          message: "Task detached",
          recoverable: true,
        }),
      ]),
    );
    expect(mapping.snapshot.chat.items.map((item) => item.message.kind)).not.toContain("permission");
    expect(mapping.snapshot.active_requests).toEqual([]);
    expect(mapping.warnings).toEqual(
      expect.arrayContaining([
        { kind: "pendingRequestsNeedNativeSurface", requestIds: ["request-1"] },
        { kind: "recoveryMappedToInterruption", actions: ["continue"] },
        { kind: "preparationNeedsNativeSurface", state: "blocked" },
        { kind: "sendCapabilityNeedsNativeSurface", state: "blocked" },
        { kind: "agentCommandsNeedNativeSurface", state: "loading" },
      ]),
    );
    expect(mapping.requiresNativeSurface).toBe(true);
  });

  it("maps task-scoped App Server permission snapshots to answerable permission cards", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot({
      pendingRequests: [
        {
          requestId: "request-1" as RequestId,
          kind: "permission",
          title: "Allow command?",
          scope: { kind: "task", taskId: "task-1" as TaskId },
          permission: {
            title: "Allow command?",
            description: "Run command",
            scope: "workspace",
            risk: "writes files",
            toolCall: { id: "tool-1", title: "Shell command", kind: "execute" },
            options: [
              { optionId: "allow-once", name: "Allow", kind: "allowOnce" },
              { optionId: "reject-once", name: "Deny", kind: "rejectOnce" },
            ],
          },
        },
      ],
    }));

    expect(mapping.snapshot.active_requests.map((item) => item.message)).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          kind: "permission",
          request_id: "request-1",
          app_server_request_id: "request-1",
          title: "Allow command?",
          description: "Run command",
          scope: "workspace",
          risk: "writes files",
          tool_call: { id: "tool-1", title: "Shell command", kind: "execute" },
          state: "pending",
          options: [
            { id: "allow-once", label: "Allow", kind: "allow" },
            { id: "reject-once", label: "Deny", kind: "deny" },
          ],
        }),
      ]),
    );
    expect(mapping.snapshot.chat.items.map((item) => item.message.kind)).not.toContain("permission");
    expect(mapping.snapshot.chat.items.map((item) => item.message.kind)).not.toContain("interruption");
    expect(mapping.warnings).not.toEqual(
      expect.arrayContaining([{ kind: "pendingRequestsNeedNativeSurface", requestIds: ["request-1"] }]),
    );
    expect(mapping.requiresNativeSurface).toBe(false);
  });

  it("maps saved permission outcomes onto their tool activity", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot({
      chat: {
        hasMessages: true,
        items: [
          {
            messageId: "activity-1" as MessageId,
            role: "system",
            status: "complete",
            parts: [
              {
                kind: "activity",
                title: "Tool call",
                status: "completed",
                steps: [{
                  kind: "tool",
                  toolCallId: "call-1",
                  name: "execute",
                  status: "completed",
                  permissionOutcomes: [{
                    requestId: "server-request-1" as RequestId,
                    decision: "rejected",
                    optionId: "reject_once",
                    optionLabel: "Reject",
                    resolvedAt: "2026-01-01T00:00:02.000Z",
                  }],
                }],
              },
            ],
          },
        ],
      },
    }));

    expect(mapping.snapshot.chat.items[0].message).toMatchObject({
      kind: "activity",
      title: "Tool call",
      steps: [{
        kind: "tool",
        tool_call_id: "call-1",
        status: "completed",
        permission_outcomes: [{
          request_id: "server-request-1",
          decision: "rejected",
          option_id: "reject_once",
          option_label: "Reject",
          resolved_at: "2026-01-01T00:00:02.000Z",
        }],
      }],
    });
  });

  it("maps pending and resolved Questions through the typed chat seam", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot({
      pendingRequests: [{
        requestId: "question-pending" as RequestId,
        kind: "question",
        title: "Question",
        scope: { kind: "task", taskId: "task-1" as TaskId },
        question: {
          message: "Choose a scope.",
          fields: [{ kind: "string", key: "name", title: "Name", required: true }],
        },
      }],
      chat: {
        hasMessages: true,
        items: [{
          messageId: "question-resolved" as MessageId,
          role: "system",
          status: "complete",
          parts: [{
            kind: "question",
            requestId: "question-resolved-request" as RequestId,
            message: "Choose a scope.",
            fields: [{
              kind: "singleSelect",
              key: "scope",
              title: "Scope",
              required: true,
              options: [{ value: "form", label: "Form only" }],
            }],
            state: "resolved",
            action: "submit",
            content: { scope: "form" },
          }],
        }],
      },
    }));

    expect(mapping.snapshot.chat.items.map((item) => item.message)).toEqual(expect.arrayContaining([
      expect.objectContaining({
        kind: "elicitation",
        request_id: "question-resolved-request",
        state: "resolved",
        answers: [{ field_id: "scope", label: "Scope", value: "Form only" }],
      }),
    ]));
    expect(mapping.snapshot.active_requests.map((item) => item.message)).toEqual([
      expect.objectContaining({
        kind: "elicitation",
        request_id: "question-pending",
        app_server_request_id: "question-pending",
        state: "pending",
      }),
    ]);
    expect(mapping.warnings).not.toEqual(
      expect.arrayContaining([{ kind: "pendingRequestsNeedNativeSurface", requestIds: ["question-pending"] }]),
    );
    expect(mapping.snapshot.active_requests.filter((item) => (
      item.message.kind === "elicitation" && item.message.request_id === "question-pending"
    ))).toHaveLength(1);
  });

  it("maps interrupted chat items to visible interruption rows", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot({
      chat: {
        hasMessages: true,
        items: [
          {
            messageId: "agent-1" as MessageId,
            role: "agent",
            status: "interrupted",
            parts: [{ kind: "text", text: "Stopped while running." }],
          },
        ],
      },
    }));

    expect(mapping.snapshot.chat.items[0].message).toMatchObject({
      kind: "interruption",
      message: "Stopped while running.",
      recoverable: true,
    });
  });

  it("hides interrupted Working boilerplate after protocol mapping", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot({
      chat: {
        hasMessages: true,
        items: [
          {
            messageId: "working" as MessageId,
            role: "system",
            status: "interrupted",
            parts: [{
              kind: "activity",
              title: "Working",
              status: "interrupted",
              steps: [{ kind: "text", text: "Started", level: "info" }],
            }],
          },
          {
            messageId: "stopped" as MessageId,
            role: "system",
            status: "interrupted",
            parts: [{ kind: "text", text: "Task was stopped." }],
          },
        ],
      },
    }));

    expect(renderedChat(mapping.snapshot, undefined).items.map((item) => item.message)).toEqual([
      expect.objectContaining({ kind: "interruption", message: "Task was stopped." }),
    ]);
  });

  it("requires native surface for pending requests even without other blockers", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot({
      pendingRequests: [
        {
          requestId: "request-1" as RequestId,
          kind: "permission",
          title: "Allow command?",
          scope: { kind: "task", taskId: "task-1" as TaskId },
        },
      ],
    }));

    expect(mapping.requiresNativeSurface).toBe(true);
    expect(mapping.warnings).toEqual(
      expect.arrayContaining([{ kind: "pendingRequestsNeedNativeSurface", requestIds: ["request-1"] }]),
    );
  });

  it("preserves failed preparation status over send blocking", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot({
      preparation: {
        kind: "failed",
        error: { code: "internal", message: "Preparation failed", recoverable: true },
        actions: ["retry"],
      },
      sendCapability: { state: "blocked", blockers: [{ kind: "taskPreparing", message: "Still preparing" }] },
    }));

    expect(mapping.snapshot.task.status).toBe("failed");
    expect(mapping.requiresNativeSurface).toBe(true);
  });

  it("does not surface active-turn send blocking as a user-facing task blocker", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot({
      task: protocolSummary({ status: "running" }),
      sendCapability: { state: "blocked", blockers: [{ kind: "taskRunning", message: "Task is already running" }] },
    }));

    expect(mapping.snapshot.task.status).toBe("active");
    expect(mapping.snapshot.send_capability.blockers).toEqual([
      { kind: "taskRunning", message: "Task is already running" },
    ]);
    expect(mapping.snapshot.chat.items.map((item) => item.message)).not.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          kind: "interruption",
          message: "Task is already running",
        }),
      ]),
    );
    expect(mapping.warnings).not.toEqual(
      expect.arrayContaining([{ kind: "sendCapabilityNeedsNativeSurface", state: "blocked" }]),
    );
  });

  it("does not render the normal starting-state send blocker in Chat", () => {
    const mapping = mapProtocolTaskSnapshot(protocolSnapshot({
      task: protocolSummary({ status: "starting" }),
      sendCapability: { state: "blocked", blockers: [{ kind: "taskRunning", message: "Task is already running" }] },
    }));

    expect(mapping.snapshot.send_capability.blockers).toEqual([
      { kind: "taskRunning", message: "Task is already running" },
    ]);
    expect(mapping.snapshot.chat.items.map((item) => item.message)).not.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          kind: "interruption",
          message: "Task is already running",
        }),
      ]),
    );
  });
});

function protocolSnapshot(overrides: Partial<ProtocolTaskSnapshot> = {}): ProtocolTaskSnapshot {
  return {
    task: protocolSummary({ status: "preparing" }),
    lifecycle: "visible",
    revision: 7,
    preparation: { kind: "ready" },
    agentConfig: {
      state: "ready",
      options: [
        {
          configId: "model" as never,
          label: "Model",
          category: "model",
          kind: "select",
          currentValue: "gpt-5",
          values: [{ value: "gpt-5", label: "GPT-5" }],
        },
      ],
    },
    agentCommands: { state: "ready", commands: [] },
    sendCapability: { state: "ready" },
    historySync: { state: "idle", generation: 0 },
    chat: {
      hasMoreBefore: true,
      hasMessages: true,
      startCursor: "m:1" as MessageId,
      endCursor: "m:3" as MessageId,
      items: [
        {
          messageId: "user-1" as MessageId,
          role: "user",
          status: "complete",
          parts: [
            { kind: "text", text: "hello" },
            {
              kind: "attachment",
              attachment: {
                attachmentId: "attachment-1" as never,
                kind: "fileReference",
                label: "README.md",
              },
            },
          ],
        },
        {
          messageId: "agent-1" as MessageId,
          role: "agent",
          status: "streaming",
          parts: [{ kind: "text", text: "world" }],
        },
        {
          messageId: "activity-1" as MessageId,
          role: "agent",
          status: "complete",
          parts: [
            {
              kind: "activity",
              title: "Running tests",
              status: "failed",
              steps: [
                {
                  kind: "command",
                  commandLabel: "npm test",
                  status: "failed",
                  exitCode: 1,
                  outputPreview: "failed",
                },
                {
                  kind: "tool",
                  toolCallId: "tool-1" as RequestId,
                  name: "edit",
                  status: "completed",
                  inputSummary: "Editing files",
                  detailArtifactId: "artifact_1",
                  permissionOutcomes: [],
                },
              ],
            },
          ],
        },
      ],
    },
    ...overrides,
  };
}

function protocolSummary(overrides: Partial<ProtocolTaskSummary> = {}): ProtocolTaskSummary {
  return {
    taskId: "task-1" as TaskId,
    projectId: "project-1" as ProjectId,
    agentId: "codex" as AgentId,
    title: { value: "Task", source: "user" },
    status: "idle" as const,
    updatedAt: "2026-06-27T12:00:00.000Z",
    lastActivity: "2026-06-27T12:00:00.000Z",
    unread: false,
    hasMessages: true,
    ...overrides,
  };
}

function mappingContext() {
  return {
    agents: [{ agentId: "codex" as AgentId, label: "Codex", status: "connected" as const }],
    projects: [{ projectId: "project-1" as ProjectId, label: "App" }],
  };
}
