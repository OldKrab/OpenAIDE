import { afterEach, describe, expect, it, vi } from "vitest";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import {
  ATTACHMENT_RELEASE,
  TASK_SEND,
  type AttachmentHandleId,
} from "@openaide/app-server-client";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("task mutation intents", () => {
  it("issues one task/send without a recovery identity", async () => {
    const { sendTaskPromptIntent } = await import("./taskMutationIntents");
    const request = vi.fn().mockRejectedValue(new Error("connection closed"));
    const input = { prompt: "Do this once", context: [] };
    const dependencies = {
      backendConnection: { request },
      clientInstanceId: "client-a",
      createSnapshotRequestId: vi.fn(() => 1),
      dispatch: vi.fn(),
      postHostMessage: vi.fn(),
      stateRootId: "root-a",
    };

    sendTaskPromptIntent(dependencies, taskSnapshot(), input);
    await vi.waitFor(() => expect(request).toHaveBeenCalledTimes(1));
    await vi.waitFor(() => expect(dependencies.dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:sendError",
      taskId: "task-a",
    })));
    expect(request).toHaveBeenCalledWith(TASK_SEND, {
      taskId: "task-a",
      taskRevision: 1,
      message: { text: "Do this once" },
    });
  });

  it("does not issue task/send while the authoritative capability is blocked", async () => {
    const { sendTaskPromptIntent } = await import("./taskMutationIntents");
    const request = vi.fn();
    const dispatch = vi.fn();
    const snapshot = taskSnapshot();
    snapshot.send_capability = {
      state: "blocked",
      blockers: [{ kind: "taskRunning", message: "Task is already running" }],
    };

    sendTaskPromptIntent({
      backendConnection: { request },
      clientInstanceId: "client-a",
      createSnapshotRequestId: vi.fn(() => 1),
      dispatch,
      postHostMessage: vi.fn(),
      stateRootId: "root-a",
    }, snapshot, { prompt: "Draft follow-up", context: [] });

    expect(request).not.toHaveBeenCalled();
    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:error",
      taskId: "task-a",
      message: "Task is already running",
    });
  });

  it("does not retry task/send after a revision rejection", async () => {
    const { sendTaskPromptIntent } = await import("./taskMutationIntents");
    const { AppServerProtocolError } = await import("@openaide/app-server-client");
    let sendCount = 0;
    const request = vi.fn(async (method: string, _params?: unknown) => {
      if (method === TASK_SEND) {
        sendCount += 1;
        throw new AppServerProtocolError({
          error: {
            code: "conflict",
            message: sendCount === 1 ? "Revision changed" : "Task is active",
            recoverable: true,
            target: sendCount === 1 ? {
              field: "taskRevision",
              currentTask: { task: { taskId: "task-a" }, revision: 9 } as never,
            } : undefined,
          },
        });
      }
      throw new Error(method);
    });

    sendTaskPromptIntent({
      backendConnection: { request: request as never },
      clientInstanceId: "client-revision",
      createSnapshotRequestId: vi.fn(() => 1),
      dispatch: vi.fn(),
      postHostMessage: vi.fn(),
      stateRootId: "root-a",
    }, taskSnapshot(), { prompt: "Follow up once", context: [] });
    await vi.waitFor(() => expect(sendCount).toBe(1));

    const sends = request.mock.calls.filter(([method]) => method === TASK_SEND);
    expect(request.mock.calls.map(([method]) => method)).toEqual([TASK_SEND]);
    expect(sends[0]?.[1]).toMatchObject({ taskRevision: 1 });
  });

  it("releases every abandoned handle after an authoritative attachment rejection", async () => {
    vi.resetModules();
    const { sendTaskPromptIntent } = await import("./taskMutationIntents");
    const { AppServerProtocolError } = await import("@openaide/app-server-client");
    const request = vi.fn(async (method: string) => {
      if (method === TASK_SEND) {
        throw new AppServerProtocolError({
          error: {
            code: "attachmentHandleInvalid",
            message: "Attachment is no longer available.",
            recoverable: true,
            target: { field: "attachments" },
          },
        });
      }
      if (method === ATTACHMENT_RELEASE) {
        return { outcomes: [] };
      }
      throw new Error(method);
    });
    const input = {
      prompt: "Inspect these",
      context: ["handle-1", "handle-2"].map((handleId) => ({
        kind: "file" as const,
        label: `${handleId}.txt`,
        local_id: `local-${handleId}`,
        app_server_handle_id: handleId as AttachmentHandleId,
      })),
    };
    const dispatch = vi.fn();

    sendTaskPromptIntent({
      backendConnection: { request: request as never },
      clientInstanceId: "client-a",
      createSnapshotRequestId: vi.fn(() => 1),
      dispatch,
      postHostMessage: vi.fn(),
      stateRootId: "root-a",
    }, taskSnapshot(), input);
    await vi.waitFor(() => expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:attachments:invalidate",
    })));
    expect(request).toHaveBeenCalledTimes(2);

    expect(request).toHaveBeenLastCalledWith(ATTACHMENT_RELEASE, {
      taskId: "task-a",
      resources: [
        { kind: "handle", id: "handle-1" },
        { kind: "handle", id: "handle-2" },
      ],
    });
  });


});

function taskSnapshot(): TaskSnapshot {
  return {
    lifecycle: "visible",
    task: {
      task_id: "task-a",
      project_id: "project-a",
      title: "Task",
      status: "inactive",
      task_version: 1,
      message_history_version: 1,
      has_messages: true,
      unread: false,
      created_at: "2026-07-12T00:00:00Z",
      updated_at: "2026-07-12T00:00:00Z",
      last_activity: "2026-07-12T00:00:00Z",
      agent_id: "codex",
      agent_name: "Codex",
      isolation: "local",
      workspace_root: "/workspace",
    },
    chat: {
      task_id: "task-a",
      items: [],
      has_before: false,
      has_messages: true,
      total_count: 0,
      version: 1,
    },
    permissions: [],
    settings_summary: { agent_id: "codex", isolation: "local" },
    send_capability: { state: "ready" },
    revision: 1,
    history_sync: { state: "idle", generation: 0 },
  };
}
