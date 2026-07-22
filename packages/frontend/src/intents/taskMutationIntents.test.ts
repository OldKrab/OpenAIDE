import { afterEach, describe, expect, it, vi } from "vitest";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import {
  TASK_SEND,
  type AttachmentHandleId,
} from "@openaide/app-server-client";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("task mutation intents", () => {
  it("sends only the Task identity, message text, and attachments", async () => {
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
      message: { text: "Do this once" },
    });
  });

  it("does not expose an HTML intermediary response as the send error", async () => {
    const { sendTaskPromptIntent } = await import("./taskMutationIntents");
    const dispatch = vi.fn();
    const request = vi.fn().mockRejectedValue(new Error(
      "App Server reliable-session upload failed with HTTP 403: <!doctype html><html>proxy</html>",
    ));

    sendTaskPromptIntent({
      backendConnection: { request },
      clientInstanceId: "client-a",
      createSnapshotRequestId: vi.fn(() => 1),
      dispatch,
      postHostMessage: vi.fn(),
      stateRootId: "root-a",
    }, taskSnapshot(), { prompt: "Explain this", context: [] });

    await vi.waitFor(() => expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:sendError",
      taskId: "task-a",
      message: "Unable to send message.",
    }));
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

  it("sends client-owned Images inline without creating an attachment resource", async () => {
    const { sendTaskPromptIntent } = await import("./taskMutationIntents");
    const request = vi.fn().mockRejectedValue(new Error("connection closed"));
    const input = {
      prompt: "Explain this",
      context: [{
        kind: "image" as const,
        label: "pasted.png",
        local_id: "image-1",
        preview_url: "data:image/png;base64,AQID",
        payload: { data: "AQID", mimeType: "image/png" },
      }],
    };

    sendTaskPromptIntent({
      backendConnection: { request },
      clientInstanceId: "client-a",
      createSnapshotRequestId: vi.fn(() => 1),
      dispatch: vi.fn(),
      postHostMessage: vi.fn(),
      stateRootId: "root-a",
    }, taskSnapshot(), input);

    await vi.waitFor(() => expect(request).toHaveBeenCalledWith(TASK_SEND, {
      taskId: "task-a",
      message: {
        text: "Explain this",
        images: [{ label: "pasted.png", mimeType: "image/png", data: "AQID" }],
      },
    }));
  });

  it("sends general files as opaque attachment handles", async () => {
    const { sendTaskPromptIntent } = await import("./taskMutationIntents");
    const request = vi.fn().mockRejectedValue(new Error("connection closed"));
    const input = {
      prompt: "",
      context: [{
        kind: "file" as const,
        label: "large-model.bin",
        local_id: "file-1",
        app_server_handle_id: "attachment-1" as AttachmentHandleId,
      }],
    };

    sendTaskPromptIntent({
      backendConnection: { request },
      clientInstanceId: "client-a",
      createSnapshotRequestId: vi.fn(() => 1),
      dispatch: vi.fn(),
      postHostMessage: vi.fn(),
      stateRootId: "root-a",
    }, taskSnapshot(), input);

    await vi.waitFor(() => expect(request).toHaveBeenCalledWith(TASK_SEND, {
      taskId: "task-a",
      message: { text: "", attachments: ["attachment-1"] },
    }));
  });

  it("does not retry a rejected task/send", async () => {
    const { sendTaskPromptIntent } = await import("./taskMutationIntents");
    const { AppServerProtocolError } = await import("@openaide/app-server-client");
    let sendCount = 0;
    const request = vi.fn(async (method: string, _params?: unknown) => {
      if (method === TASK_SEND) {
        sendCount += 1;
        throw new AppServerProtocolError({
          error: {
            code: "conflict",
            message: sendCount === 1 ? "Task is active" : "Unexpected retry",
            recoverable: true,
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
    expect(sends[0]?.[1]).toEqual({ taskId: "task-a", message: { text: "Follow up once" } });
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
    active_requests: [],
    settings_summary: { agent_id: "codex", isolation: "local" },
    send_capability: { state: "ready" },
    revision: 1,
    history_sync: { state: "idle", generation: 0 },
  };
}
