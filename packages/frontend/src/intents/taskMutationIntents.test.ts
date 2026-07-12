import { afterEach, describe, expect, it, vi } from "vitest";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import {
  ATTACHMENT_RELEASE,
  TASK_SEND,
  type AttachmentHandleId,
} from "@openaide/app-server-client";

const originalCrypto = globalThis.crypto;

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("task mutation intents", () => {
  it("creates send idempotency keys that do not reset to durable receipt keys after reload", async () => {
    vi.stubGlobal("crypto", { ...originalCrypto, randomUUID: vi.fn(() => "uuid-1") });
    vi.resetModules();
    const firstModule = await import("./taskMutationIntents");

    expect(firstModule.createTaskSendIdempotencyKey()).toBe("frontend-send-uuid-1");

    vi.stubGlobal("crypto", { ...originalCrypto, randomUUID: vi.fn(() => "uuid-2") });
    vi.resetModules();
    const reloadedModule = await import("./taskMutationIntents");

    expect(reloadedModule.createTaskSendIdempotencyKey()).toBe("frontend-send-uuid-2");
  });

  it("keeps generating usable keys when crypto.randomUUID is unavailable", async () => {
    vi.stubGlobal("crypto", {});
    vi.resetModules();
    const module = await import("./taskMutationIntents");

    expect(module.createTaskSendIdempotencyKey()).toMatch(/^frontend-send-/);
  });

  it("retries an existing Task send with the same persisted idempotency key", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
    vi.resetModules();
    const { sendTaskPromptIntent } = await import("./taskMutationIntents");
    const { readPendingTaskSendRecovery } = await import("../services/pendingTaskSendRecovery");
    const request = vi.fn()
      .mockRejectedValueOnce(new Error("connection closed"))
      .mockReturnValueOnce(new Promise(() => undefined));
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
    const recovery = readPendingTaskSendRecovery("root-a", "client-a", "task-a");
    expect(recovery?.idempotencyKey).toBe(request.mock.calls[0][1].idempotencyKey);
    await vi.waitFor(() => expect(dependencies.dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:sendUncertain",
      taskId: "task-a",
    })));

    sendTaskPromptIntent(dependencies, taskSnapshot(), input);
    await vi.waitFor(() => expect(request).toHaveBeenCalledTimes(2));
    expect(request.mock.calls[1][1].idempotencyKey).toBe(request.mock.calls[0][1].idempotencyKey);
  });

  it("retries the exact ambiguous message after a changed draft without minting another send", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
    vi.resetModules();
    const { sendTaskPromptIntent } = await import("./taskMutationIntents");
    const firstResponse = new Error("connection closed");
    const retryResponse = new Promise(() => undefined);
    const request = vi.fn()
      .mockRejectedValueOnce(firstResponse)
      .mockReturnValueOnce(retryResponse);
    const dependencies = {
      backendConnection: { request },
      clientInstanceId: "client-a",
      createSnapshotRequestId: vi.fn(() => 1),
      dispatch: vi.fn(),
      postHostMessage: vi.fn(),
      stateRootId: "root-a",
    };

    sendTaskPromptIntent(dependencies, taskSnapshot(), { prompt: "Original message", context: [] });
    await vi.waitFor(() => expect(request).toHaveBeenCalledTimes(1));
    await vi.waitFor(() => expect(dependencies.dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:sendUncertain",
      taskId: "task-a",
    })));

    sendTaskPromptIntent(dependencies, taskSnapshot(), { prompt: "Edited draft", context: [] });
    await vi.waitFor(() => expect(request).toHaveBeenCalledTimes(2));

    expect(request.mock.calls[1][1]).toMatchObject({
      idempotencyKey: request.mock.calls[0][1].idempotencyKey,
      message: { text: "Original message" },
    });
    expect(dependencies.dispatch).toHaveBeenLastCalledWith(expect.objectContaining({
      type: "taskInput:submit",
      input: { prompt: "Original message", context: [] },
      idempotencyKey: request.mock.calls[0][1].idempotencyKey,
    }));
  });

  it("does not issue task/send while the authoritative capability is blocked", async () => {
    const { sendTaskPromptIntent } = await import("./taskMutationIntents");
    const request = vi.fn();
    const dispatch = vi.fn();
    const snapshot = taskSnapshot();
    snapshot.send_capability = {
      state: "blocked",
      attachment_only: false,
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

  it("refreshes revision once and retries an established Task with the same send identity", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
    vi.resetModules();
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
    await vi.waitFor(() => expect(sendCount).toBe(2));

    const sends = request.mock.calls.filter(([method]) => method === TASK_SEND);
    const firstSend = sends[0]?.[1] as { idempotencyKey: string };
    expect(request.mock.calls.map(([method]) => method)).toEqual([TASK_SEND, TASK_SEND]);
    expect(sends[0]?.[1]).toMatchObject({ taskRevision: 1 });
    expect(sends[1]?.[1]).toMatchObject({
      taskRevision: 9,
      idempotencyKey: firstSend.idempotencyKey,
      message: { text: "Follow up once" },
    });
  });

  it("releases every abandoned handle after an authoritative attachment rejection", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
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

  it("releases the exact recovered attachments when an ambiguous retry is rejected", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
    vi.resetModules();
    const { sendTaskPromptIntent } = await import("./taskMutationIntents");
    const { AppServerProtocolError } = await import("@openaide/app-server-client");
    let sendCount = 0;
    const request = vi.fn(async (method: string) => {
      if (method === TASK_SEND) {
        sendCount += 1;
        if (sendCount === 1) throw new Error("connection closed");
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
    const dispatch = vi.fn();
    const dependencies = {
      backendConnection: { request: request as never },
      clientInstanceId: "client-exact-attachment-retry",
      createSnapshotRequestId: vi.fn(() => 1),
      dispatch,
      postHostMessage: vi.fn(),
      stateRootId: "root-a",
    };
    const attachment = (handleId: string) => ({
      kind: "file" as const,
      label: `${handleId}.txt`,
      local_id: `local-${handleId}`,
      app_server_handle_id: handleId as AttachmentHandleId,
    });

    sendTaskPromptIntent(dependencies, taskSnapshot(), {
      prompt: "Original message",
      context: [attachment("original-handle")],
    });
    await vi.waitFor(() => expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:sendUncertain",
    })));

    sendTaskPromptIntent(dependencies, taskSnapshot(), {
      prompt: "Edited draft",
      context: [attachment("edited-handle")],
    });
    await vi.waitFor(() => expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:attachments:invalidate",
    })));

    expect(request).toHaveBeenLastCalledWith(ATTACHMENT_RELEASE, {
      taskId: "task-a",
      resources: [{ kind: "handle", id: "original-handle" }],
    });
  });
});

function taskSnapshot(): TaskSnapshot {
  return {
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
    send_capability: { state: "ready", attachment_only: false },
    revision: 1,
    history_sync: { state: "idle", generation: 0 },
  };
}

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => {
      values.delete(key);
    },
    setItem: (key, value) => {
      values.set(key, value);
    },
  };
}
