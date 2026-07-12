import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatMessage } from "@openaide/app-shell-contracts";

type PermissionChatMessage = ChatMessage & {
  message: Extract<ChatMessage["message"], { kind: "permission" }>;
};

describe("TaskView permission response state", () => {
  beforeEach(() => {
    vi.stubGlobal("window", { acquireVsCodeApi: undefined });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("keys App Server permission response state by server-request identity", async () => {
    const { permissionResponseForMessage } = await import("./TaskView");
    const message = permissionMessage("agent-request-1", "server-request-1");

    expect(
      permissionResponseForMessage(message.message, {
        "agent-request-1": { responding: false, error: "wrong key" },
        "server-request-1": { responding: true },
      }),
    ).toEqual({ responding: true });
  });

  it("keeps a permission row stable when live state is replaced by persisted history", async () => {
    const { chatRowKey } = await import("./TaskView");
    const live = permissionMessage(
      "agent-request-1",
      "server-request-1",
      "tool-1",
      "app-server-permission-server-request-1",
    );
    const persisted = resolvedPermissionMessage(
      "agent-request-1",
      "server-request-1",
      "tool-1",
      "persisted-permission-1",
    );

    expect(chatRowKey(live)).toBe(chatRowKey(persisted));
  });

  it("replaces thin pending App Server request markers with delivered permission cards", async () => {
    const { chatItemsWithAppServerPermissions } = await import("./TaskView");
    const permission = permissionMessage("agent-request-1", "server-request-1");

    const items = chatItemsWithAppServerPermissions(
      [
        {
          cursor: "pending-server-request-1",
          identity: "pending-server-request-1",
          message_id: "pending-server-request-1",
          message_type: "interruption",
          message: {
            kind: "interruption",
            id: "pending-server-request-1",
            reason: "backend_unavailable",
            message: "Permission needed",
            created_at: "2026-06-27T00:00:00Z",
            recoverable: true,
          },
        },
      ],
      { "server-request-1": { taskId: "task_1", message: permission } },
      "task_1",
    );

    expect(items).toEqual([permission]);
  });

  it("appends delivered App Server permission cards for the active task until the snapshot catches up", async () => {
    const { chatItemsWithAppServerPermissions } = await import("./TaskView");
    const permission = permissionMessage("server-request-1", "server-request-1");
    const activity = activityMessage("activity-1");

    const items = chatItemsWithAppServerPermissions(
      [activity],
      { "server-request-1": { taskId: "task_1", message: permission } },
      "task_1",
    );

    expect(items).toEqual([activity, permission]);
  });

  it("renders a pending permission card instead of a duplicate one-step command activity", async () => {
    const { chatItemsWithAppServerPermissions } = await import("./TaskView");
    const activity = activityMessage("activity-1", "tool-1");
    const agentText = agentMessage("agent-1");
    const permission = permissionMessage("agent-request-1", "server-request-1", "tool-1");

    const items = chatItemsWithAppServerPermissions(
      [activity, agentText],
      { "server-request-1": { taskId: "task_1", message: permission } },
      "task_1",
    );

    expect(items.map((item) => item.message_id)).toEqual(["message-1", "agent-1"]);
  });

  it("keeps resolved permission history next to the matching command activity", async () => {
    const { chatItemsWithAppServerPermissions } = await import("./TaskView");
    const activity = activityMessage("activity-1", "tool-1");
    const permission = resolvedPermissionMessage("agent-request-1", "server-request-1", "tool-1");

    const items = chatItemsWithAppServerPermissions(
      [activity, permission],
      {},
      "task_1",
    );

    expect(items.map((item) => item.message_id)).toEqual(["activity-1", "message-1"]);
  });

  it("blocks the task composer until App Server initialize completes", async () => {
    const { taskComposerAvailability } = await import("./TaskView");

    expect(
      taskComposerAvailability({
        backendReady: false,
        inputPending: false,
        preparationBlocked: false,
        taskStatus: "inactive",
      }),
    ).toEqual({
      editingDisabled: true,
      sendDisabled: true,
      placeholder: "Connecting to App Server.",
    });
  });

  it("blocks the task composer while viewing an archived task", async () => {
    const { taskComposerAvailability } = await import("./TaskView");

    expect(
      taskComposerAvailability({
        archived: true,
        backendReady: true,
        inputPending: false,
        preparationBlocked: false,
        taskStatus: "inactive",
      }),
    ).toEqual({
      editingDisabled: true,
      sendDisabled: true,
      placeholder: "Restore task to send follow-up.",
    });
  });

  it("keeps the task draft editable but disables Send while the Agent turn blocks sending", async () => {
    const { taskComposerAvailability } = await import("./TaskView");

    expect(
      taskComposerAvailability({
        backendReady: true,
        inputPending: false,
        preparationBlocked: false,
        sendCapabilityState: "blocked",
        taskStatus: "active",
      }),
    ).toEqual({
      editingDisabled: false,
      sendDisabled: true,
      placeholder: "Send a follow-up",
    });
  });

  it("keeps the task draft editable but disables Send while a request blocks the Task", async () => {
    const { taskComposerAvailability } = await import("./TaskView");

    expect(
      taskComposerAvailability({
        backendReady: true,
        inputPending: false,
        preparationBlocked: false,
        sendCapabilityState: "blocked",
        taskStatus: "blocked",
      }),
    ).toEqual({
      editingDisabled: false,
      sendDisabled: true,
      placeholder: "Draft follow-up while input is pending.",
    });
  });

  it("locks an uncertain send while keeping its exact retry available", async () => {
    const { taskComposerAvailability } = await import("./TaskView");

    expect(
      taskComposerAvailability({
        backendReady: true,
        inputPending: false,
        inputUncertain: true,
        preparationBlocked: false,
        sendCapabilityState: "blocked",
        taskStatus: "active",
      }),
    ).toEqual({
      editingDisabled: true,
      sendDisabled: false,
      placeholder: "Retry this exact message.",
    });
  });

  it("explains when attachments cannot be sent without message text", async () => {
    const { composerMessageShapeError } = await import("./taskSurfaceHelpers");

    expect(composerMessageShapeError({
      attachmentCount: 1,
      otherwiseSendable: true,
      prompt: "",
    })).toBe("Add a message for this Agent.");
    expect(composerMessageShapeError({
      attachmentCount: 1,
      otherwiseSendable: false,
      prompt: "",
    })).toBeUndefined();
    expect(composerMessageShapeError({
      attachmentCount: 1,
      otherwiseSendable: true,
      prompt: "Use this file",
    })).toBeUndefined();
  });

  it("does not append delivered App Server permission cards for another task", async () => {
    const { chatItemsWithAppServerPermissions } = await import("./TaskView");
    const permission = permissionMessage("server-request-1", "server-request-1");
    const activity = activityMessage("activity-1");

    const items = chatItemsWithAppServerPermissions(
      [activity],
      { "server-request-1": { taskId: "task_2", message: permission } },
      "task_1",
    );

    expect(items).toEqual([activity]);
  });

  it("does not guess a task for unscoped live App Server permissions", async () => {
    const { chatItemsWithAppServerPermissions } = await import("./TaskView");
    const permission = permissionMessage("server-request-1", "server-request-1");
    const activity = activityMessage("activity-1");

    const items = chatItemsWithAppServerPermissions(
      [activity],
      { "server-request-1": { message: permission } },
      "task_1",
    );

    expect(items).toEqual([activity]);
  });

  it("does not guess a task from matching live permission tool calls", async () => {
    const { chatItemsWithAppServerPermissions } = await import("./TaskView");
    const activity = activityMessage("activity-1", "tool-1");
    const permission = permissionMessage("server-request-1", "server-request-1", "tool-1");

    const items = chatItemsWithAppServerPermissions(
      [activity],
      { "server-request-1": { taskId: "stale-task-id", message: permission } },
      "task_1",
    );

    expect(items).toEqual([activity]);
  });

  it("keeps concurrent permissions distinct when they share a tool call id", async () => {
    const { chatItemsWithAppServerPermissions } = await import("./TaskView");
    const first = permissionMessage("agent-request-1", "server-request-1", "tool-1", "permission-1");
    const second = permissionMessage("agent-request-2", "server-request-2", "tool-1", "permission-2");

    const items = chatItemsWithAppServerPermissions(
      [first, second],
      {},
      "task_1",
    );

    expect(items.map((item) => item.message_id)).toEqual(["permission-1", "permission-2"]);
  });

  it("does not duplicate delivered App Server permission cards once the snapshot includes them", async () => {
    const { chatItemsWithAppServerPermissions } = await import("./TaskView");
    const permission = permissionMessage("server-request-1", "server-request-1");

    const items = chatItemsWithAppServerPermissions(
      [permission],
      { "server-request-1": { taskId: "task_1", message: permission } },
      "task_1",
    );

    expect(items).toEqual([permission]);
  });

  it("does not duplicate live App Server permissions when the snapshot only has the agent request id", async () => {
    const { chatItemsWithAppServerPermissions } = await import("./TaskView");
    const snapshotPermission = permissionMessage("agent-request-1", undefined, "tool-1");
    const livePermission = permissionMessage("agent-request-1", "server-request-1", "tool-1");

    const items = chatItemsWithAppServerPermissions(
      [snapshotPermission],
      { "server-request-1": { taskId: "task_1", message: livePermission } },
      "task_1",
    );

    expect(items).toEqual([livePermission]);
  });

  it("moves snapshot resolved permission cards next to their matching activity", async () => {
    const { chatItemsWithAppServerPermissions } = await import("./TaskView");
    const activity = activityMessage("activity-1", "tool-1");
    const agentText = agentMessage("agent-1");
    const permission = resolvedPermissionMessage("agent-request-1", "server-request-1", "tool-1");

    const items = chatItemsWithAppServerPermissions(
      [activity, agentText, permission],
      {},
      "task_1",
    );

    expect(items.map((item) => item.message_id)).toEqual(["activity-1", "message-1", "agent-1"]);
  });

  it("keeps a saved permission next to its matching ungrouped activity row", async () => {
    const { chatItemsWithAppServerPermissions } = await import("./TaskView");
    const activity = activityMessage("activity-1", "tool-1");
    const thought = thoughtMessage("thought-1");
    const command = activityMessage("activity-2", "tool-2");
    const permission = resolvedPermissionMessage("agent-request-1", "server-request-1", "tool-2", "permission-1");

    const items = chatItemsWithAppServerPermissions([activity, thought, command, permission], {}, "task_1");

    expect(items.map((item) => item.message_id)).toEqual([
      "activity-1",
      "thought-1",
      "activity-2",
      "permission-1",
    ]);
  });

  it("does not show a separate working status while a permission decision is pending", async () => {
    const { taskWorkingStatusLabel } = await import("./taskSurfaceHelpers");

    expect(taskWorkingStatusLabel([
      activityMessage("activity-1", "tool-1"),
      permissionMessage("agent-request-1", "server-request-1", "tool-1"),
    ], "blocked", false)).toBeUndefined();
  });

  it("does not dedupe distinct saved permissions that reuse App Server request ids", async () => {
    const { chatItemsWithAppServerPermissions, chatRowKey } = await import("./TaskView");
    const oldPermission = {
      ...permissionMessage("agent-request-old", "server-request-1", "tool-old", "permission-old"),
      message: {
        ...permissionMessage("agent-request-old", "server-request-1", "tool-old", "permission-old").message,
        state: "resolved",
        decision: "approved",
        selected_option: "allow",
      },
    } satisfies ChatMessage;
    const newPermission = permissionMessage("agent-request-new", "server-request-1", "tool-new", "permission-new");

    const items = chatItemsWithAppServerPermissions(
      [oldPermission, activityMessage("activity-new", "tool-new"), newPermission],
      {},
      "task_1",
    );

    expect(items.map((item) => item.message_id)).toEqual(["permission-old", "permission-new"]);
    expect(new Set(items.map(chatRowKey)).size).toBe(items.length);
  });

  it("keeps the same visible chat content anchored after earlier messages prepend", async () => {
    const { scrollTopAfterPrependedContent } = await import("./TaskView");

    expect(scrollTopAfterPrependedContent({
      previousScrollHeight: 1000,
      previousScrollTop: 240,
      nextScrollHeight: 1380,
    })).toBe(620);
    expect(scrollTopAfterPrependedContent({
      previousScrollHeight: 1000,
      previousScrollTop: 240,
      nextScrollHeight: 960,
    })).toBe(240);
  });

});

function activityMessage(id: string, toolCallId?: string): ChatMessage {
  return {
    cursor: id,
    identity: id,
    message_id: id,
    message_type: "activity",
    message: {
      kind: "activity",
      id,
      title: "Allow command",
      status: "running",
      created_at: "2026-06-27T00:00:00Z",
      collapsed: false,
      steps: toolCallId ? [{ kind: "tool", tool_call_id: toolCallId, name: "execute", status: "running" }] : [],
    },
  };
}

function agentMessage(id: string): ChatMessage {
  return {
    cursor: id,
    identity: id,
    message_id: id,
    message_type: "agent_text",
    message: {
      kind: "agent_text",
      id,
      text: "Working on it.",
      created_at: "2026-06-27T00:00:00Z",
    },
  };
}

function thoughtMessage(id: string): ChatMessage {
  return {
    cursor: id,
    identity: id,
    message_id: id,
    message_type: "thought",
    message: {
      kind: "thought",
      id,
      text: "Thinking.",
      created_at: "2026-06-27T00:00:00Z",
    },
  };
}

function permissionMessage(
  agentRequestId: string,
  appServerRequestId?: string,
  toolCallId = "tool-1",
  messageId = "message-1",
): PermissionChatMessage {
  return {
    cursor: messageId,
    identity: messageId,
    message_id: messageId,
    message_type: "permission",
    message: {
      kind: "permission",
      id: messageId,
      request_id: agentRequestId,
      app_server_request_id: appServerRequestId,
      title: "Allow command",
      tool_call: { id: toolCallId, title: "Allow command" },
      state: "pending",
      created_at: "2026-06-27T00:00:00Z",
      options: [{ id: "allow", label: "Allow", kind: "allow" }],
    },
  };
}

function resolvedPermissionMessage(
  agentRequestId: string,
  appServerRequestId?: string,
  toolCallId = "tool-1",
  messageId = "message-1",
): PermissionChatMessage {
  const message = permissionMessage(agentRequestId, appServerRequestId, toolCallId, messageId);
  return {
    ...message,
    message: {
      ...message.message,
      state: "resolved",
      decision: "approved",
      selected_option: "allow",
    },
  } satisfies ChatMessage;
}
