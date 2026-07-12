import { beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import {
  AGENT_AUTHENTICATE,
  AGENT_CREATE_CUSTOM,
  AGENT_DELETE_CUSTOM,
  AGENT_REPLACE_CUSTOM,
  AGENT_SET_ENABLED,
  AGENT_UPDATE_CUSTOM_METADATA,
  ATTACHMENT_CONFIRM_EMBEDDED,
  ATTACHMENT_CREATE_EMBEDDED_CANDIDATE,
  ATTACHMENT_CREATE_FILE_REFERENCE,
  ATTACHMENT_CREATE_PASTED_IMAGE,
  ATTACHMENT_LIST_DIRECTORY,
  ATTACHMENT_LIST_ROOTS,
  ATTACHMENT_RELEASE_HANDLES,
  ATTACHMENT_REVEAL,
  AppServerProtocolError,
  SETTINGS_GET_AGENT_DETAILS,
  SETTINGS_GET_MCP_SERVERS,
  SETTINGS_GET_SKILLS,
  SETTINGS_UPDATE_PREFERENCES,
  SETTINGS_UPDATE_RUNTIME,
  TASK_ADOPT_NATIVE_SESSION,
  TASK_CANCEL,
  TASK_CHAT_PAGE,
  TASK_CREATE,
  TASK_DISCARD,
  TASK_LIST,
  TASK_OPEN,
  TASK_SEND,
  TASK_SET_ARCHIVED,
  TASK_SET_CONFIG_OPTION,
  TASK_TOOL_DETAIL,
  type AttachmentHandleId,
  type BackendConnection,
  type FileBrowserEntryId,
  type FileBrowserRootId,
  type TaskSnapshot as ProtocolTaskSnapshot,
} from "@openaide/app-server-client";
import { createAppCallbacks } from "./appControllerCallbacks";
import { PENDING_TASK_SEND_RECOVERY_KEY } from "../services/pendingTaskSendRecovery";
import { projectIdForWorkspaceRoot } from "../state/projectIdentity";
import { createInitialState, toolDetailCacheKey, type AppState } from "../state/store";

const postHostMessage = vi.fn();
const beginAgentSecretTransaction = vi.fn();

vi.mock("../services/hostBridge", () => ({
  postHostMessage: (message: unknown) => postHostMessage(message),
}));

vi.mock("../services/agentSecretTransaction", () => ({
  beginAgentSecretTransaction: (changes: unknown) => beginAgentSecretTransaction(changes),
}));

describe("app controller callbacks", () => {
  beforeEach(() => {
    postHostMessage.mockClear();
    beginAgentSecretTransaction.mockReset();
    beginAgentSecretTransaction.mockResolvedValue({
      commit: vi.fn(async () => undefined),
      rollback: vi.fn(async () => undefined),
    });
  });

  it("submits selected config atomically with Draft Task creation before Send", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_CREATE) return { task: protocolTaskSnapshot("task_1", "New task") };
      if (method === TASK_SET_CONFIG_OPTION) return { task: { ...protocolTaskSnapshot("task_1", "New task"), revision: 3 } };
      if (method === TASK_SEND) return { task: { ...protocolTaskSnapshot("task_1", "New task"), revision: 4 } };
      throw new Error(method);
    });
    const state = createInitialState();
    state.newTask.prompt = "Build the thing";
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project_1",
      configOptions: { model: "gpt" },
      workspaceRoot: "/workspace",
      workspaceLabel: "workspace",
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).newTask.submit();
    await settlePromises();

    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "submit:start" });
    expect(request).toHaveBeenNthCalledWith(1, TASK_CREATE, {
      projectId: "project_1",
      agentId: "codex",
      configOptions: { model: "gpt" },
    });
    expect(request).toHaveBeenNthCalledWith(2, TASK_SEND, {
      taskId: "task_1",
      taskRevision: 2,
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
      message: { text: "Build the thing" },
    });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: "snapshot", intent: "open" }));
    expect(dispatch).toHaveBeenCalledWith({ type: "taskInput:submit", taskId: "task_1" });
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openTask",
      payload: {
        task_id: "task_1",
        title: "New task",
      },
    });
  });

  it("stops a pre-send startup by discarding its prepared Task", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_DISCARD) return { discardedTaskId: "task_1", tasks: { tasks: [], activeTaskId: null } };
      throw new Error(method);
    });
    const state = createInitialState();
    state.newTask.selection.projectId = "project_1";
    state.newTask.pending = { prompt: "Build the thing", context: [] };
    state.newTask.submitting = true;
    const attempt = {
      cancelled: false,
      draft: { prompt: "Build the thing", context: [] },
      taskId: "task_1" as never,
    };
    const newTaskStartAttempt = { current: attempt as typeof attempt | undefined };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      newTaskStartAttempt,
      state,
    }).newTask.cancel();
    await settlePromises();

    expect(attempt.cancelled).toBe(true);
    expect(newTaskStartAttempt.current).toBeUndefined();
    expect(dispatch).toHaveBeenCalledWith({ type: "submit:cancel" });
    expect(request).toHaveBeenCalledWith(TASK_DISCARD, { taskId: "task_1" });
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openNewTask",
      payload: { project_id: "project_1" },
    });
  });

  it("includes a new workspace root when creating a task for an unseen project", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_CREATE) return { task: protocolTaskSnapshot("task_1", "New task") };
      if (method === TASK_SEND) return { task: { ...protocolTaskSnapshot("task_1", "New task"), revision: 2 } };
      throw new Error(method);
    });
    const state = createInitialState();
    state.newTask.prompt = "Start here";
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project-fe42cc83da346a18",
      workspaceRoot: "/workspace/new-app",
      workspaceLabel: "new-app",
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).newTask.submit();
    await settlePromises();

    expect(request).toHaveBeenNthCalledWith(1, TASK_CREATE, {
      projectId: "project-fe42cc83da346a18",
      agentId: "codex",
      workspaceRoot: "/workspace/new-app",
    });
  });

  it("attaches a pasted image from the new-task composer by preparing the Task first", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_CREATE) return { task: protocolTaskSnapshot("task_1", "New task") };
      if (method === ATTACHMENT_CREATE_PASTED_IMAGE) {
        return { attachment: { handleId: "attachment-handle-image", label: "pasted.png" } };
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.newTask.prompt = "";
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project_1",
      workspaceRoot: "/workspace",
      workspaceLabel: "workspace",
    };

    await callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).newTask.fileBrowser?.attachPastedImage(
      new File([new Uint8Array([1, 2, 3])], "pasted.png", { type: "image/png" }),
      { prompt: "Explain this", context: [] },
    );

    expect(request).toHaveBeenNthCalledWith(1, TASK_CREATE, {
      projectId: "project_1",
      agentId: "codex",
    });
    expect(request).toHaveBeenNthCalledWith(2, ATTACHMENT_CREATE_PASTED_IMAGE, {
      taskId: "task_1",
      label: "pasted.png",
      mimeType: "image/png",
      data: "AQID",
    });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: "snapshot", intent: "open" }));
    expect(dispatch).toHaveBeenCalledWith({ type: "taskInput:prompt", taskId: "task_1", prompt: "Explain this" });
    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:attachment:addAppServer",
      taskId: "task_1",
      attachment: expect.objectContaining({
        app_server_handle_id: "attachment-handle-image",
        label: "pasted.png",
        preview_url: "data:image/png;base64,AQID",
      }),
    });
    expect(request).not.toHaveBeenCalledWith(TASK_SEND, expect.anything());
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("does not wait for send readiness before attaching a pasted image to a prepared new Task", async () => {
    const dispatch = vi.fn();
    const neverReady = deferred<{ task: ReturnType<typeof protocolPreparingTaskSnapshot> }>();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_CREATE) return { task: protocolPreparingTaskSnapshot("task_1", "New task") };
      if (method === TASK_OPEN) return neverReady.promise;
      if (method === ATTACHMENT_CREATE_PASTED_IMAGE) {
        return { attachment: { handleId: "attachment-handle-image", label: "pasted.png" } };
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project_1",
      workspaceRoot: "/workspace",
      workspaceLabel: "workspace",
    };

    void callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).newTask.fileBrowser?.attachPastedImage(new File([new Uint8Array([1, 2, 3])], "pasted.png", { type: "image/png" }));
    await settlePromises();
    await settlePromises();

    expect(request).toHaveBeenCalledWith(ATTACHMENT_CREATE_PASTED_IMAGE, {
      taskId: "task_1",
      label: "pasted.png",
      mimeType: "image/png",
      data: "AQID",
    });
  });

  it("sends pasted image attachments from a prepared new Task", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_OPEN) return { task: protocolTaskSnapshot("task_1", "New task") };
      if (method === TASK_SEND) return { task: { ...protocolTaskSnapshot("task_1", "New task"), revision: 2 } };
      throw new Error(method);
    });
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.snapshot.task.has_messages = false;
    state.newTask.prompt = "stale text";
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project_1",
      workspaceRoot: "/workspace",
      workspaceLabel: "workspace",
    };
    state.taskInputs.task_1 = {
      prompt: "Explain this image",
      context: [{
        kind: "file",
        label: "pasted.png",
        local_id: "attachment_1",
        app_server_handle_id: "attachment-handle-image" as never,
        preview_url: "data:image/png;base64,AQID",
      }],
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).newTask.submit();
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_OPEN, { taskId: "task_1" });
    expect(request).toHaveBeenCalledWith(TASK_SEND, {
      taskId: "task_1",
      taskRevision: 2,
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
      message: {
        text: "Explain this image",
        attachments: ["attachment-handle-image"],
      },
    });
    expect(dispatch).toHaveBeenCalledWith({ type: "taskInput:submit", taskId: "task_1" });
  });

  it("invalidates prepared new-task attachments when App Server continuity is lost", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_OPEN) return { task: protocolTaskSnapshot("task_1", "New task") };
      if (method === TASK_SEND) {
        throw new AppServerProtocolError({
          error: {
            code: "attachmentHandleInvalid",
            message: "Attachment is no longer available. Reselect it and try again.",
            recoverable: true,
            target: { field: "attachments" },
          },
        });
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.snapshot.task.has_messages = false;
    state.newTask.selection = {
      ...state.newTask.selection,
      projectId: "project_1",
      workspaceRoot: "/workspace",
    };
    state.taskInputs.task_1 = {
      prompt: "Explain this image",
      context: [{
        kind: "file",
        label: "pasted.png",
        local_id: "attachment_1",
        app_server_handle_id: "attachment-handle-image" as AttachmentHandleId,
      }],
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).newTask.submit();
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({
      type: "submit:attachments:invalidate",
      taskId: "task_1",
      message: "Attachment is no longer available. Reselect it and try again.",
    });
  });

  it("sends every visible draft attachment from a prepared new Task", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_OPEN) return { task: protocolTaskSnapshot("task_1", "New task") };
      if (method === TASK_SEND) return { task: { ...protocolTaskSnapshot("task_1", "New task"), revision: 2 } };
      throw new Error(method);
    });
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.snapshot.task.has_messages = false;
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project_1",
      workspaceRoot: "/workspace",
      workspaceLabel: "workspace",
    };
    state.taskInputs.task_1 = {
      prompt: "stale prepared input",
      context: [{
        kind: "file",
        label: "first.png",
        local_id: "attachment_1",
        app_server_handle_id: "attachment-handle-1" as AttachmentHandleId,
      }],
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).newTask.submit({
      prompt: "send the visible attachments",
      context: [
        {
          kind: "file",
          label: "first.png",
          local_id: "attachment_1",
          app_server_handle_id: "attachment-handle-1" as AttachmentHandleId,
        },
        {
          kind: "file",
          label: "second.png",
          local_id: "attachment_2",
          app_server_handle_id: "attachment-handle-2" as AttachmentHandleId,
        },
        {
          kind: "file",
          label: "third.png",
          local_id: "attachment_3",
          app_server_handle_id: "attachment-handle-3" as AttachmentHandleId,
        },
      ],
    });
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SEND, {
      taskId: "task_1",
      taskRevision: 2,
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
      message: {
        text: "send the visible attachments",
        attachments: ["attachment-handle-1", "attachment-handle-2", "attachment-handle-3"],
      },
    });
  });

  it("keeps New Task visible until Send returns the authoritative user message", async () => {
    const dispatch = vi.fn();
    const pendingSend = deferred<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const request = vi.fn((method: string) => {
      if (method === TASK_CREATE) return Promise.resolve({ task: protocolTaskSnapshot("task_1", "New task") });
      if (method === TASK_SEND) return pendingSend.promise;
      return Promise.reject(new Error(method));
    });
    const state = createInitialState();
    state.newTask.prompt = "Build the thing";
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project_1",
      workspaceRoot: "/workspace",
      workspaceLabel: "workspace",
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).newTask.submit();
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SEND, expect.objectContaining({ taskId: "task_1" }));
    expect(dispatch).toHaveBeenCalledWith({ type: "taskInput:submit", taskId: "task_1" });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({
      type: "surface.openTask",
    }));

    pendingSend.resolve({
      task: {
        ...protocolTaskSnapshot("task_1", "Accepted task"),
        revision: 3,
        chat: {
          items: [{
            messageId: "user-message" as never,
            role: "user" as const,
            status: "complete" as const,
            parts: [{ kind: "text" as const, text: "Build the thing" }],
          }],
          hasMoreBefore: false,
          hasMessages: true,
        },
      },
    });
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "snapshot",
      snapshot: expect.objectContaining({
        chat: expect.objectContaining({
          items: [expect.objectContaining({ message: expect.objectContaining({ text: "Build the thing" }) })],
        }),
      }),
    }));
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openTask",
      payload: {
        task_id: "task_1",
        title: "Accepted task",
      },
    });
  });

  it("routes existing tasks for the destination surface to open", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_OPEN) {
        return { task: protocolTaskSnapshot("task_1", "Unread task") };
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.tasks = [{
      agent_id: "codex",
      agent_name: "Codex",
      created_at: "2026-05-22T00:00:00.000Z",
      has_messages: true,
      isolation: "local",
      last_activity: "2026-05-22T00:00:00.000Z",
      message_history_version: 1,
      project_id: "project_1",
      project_label: "OpenAIDE",
      status: "inactive",
      task_id: "task_1",
      task_version: 1,
      title: "Unread task",
      unread: true,
      updated_at: "2026-05-22T00:00:00.000Z",
      workspace_root: "/workspace",
    }];

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).navigation.openTask("task_1");
    await settlePromises();

    expect(request).not.toHaveBeenCalled();
    expect(dispatch).toHaveBeenCalledWith({ type: "selection:set", taskId: "task_1" });
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openTask",
      payload: { task_id: "task_1", title: "Unread task" },
    });
  });

  it("refreshes the current task snapshot when manually refreshing native sessions", async () => {
    const dispatch = vi.fn();
    const requestNativeSessions = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_OPEN) {
        return { task: protocolTaskSnapshot("task_1", "Updated adopted task") };
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.activeTaskId = "task_1";
    state.snapshot = snapshot("task_1");
    state.newTask.nativeSessions.items = [
      { session_id: "session_1", cwd: "/workspace", title: "Recent" },
      { session_id: "session_2", cwd: "/workspace", title: "Older" },
    ];

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      requestNativeSessions,
      state,
    }).navigation.loadNativeSessions();
    await settlePromises();

    expect(requestNativeSessions).toHaveBeenCalledWith(undefined, false, 2);
    expect(request).toHaveBeenCalledWith(TASK_OPEN, { taskId: "task_1" });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "snapshot",
      intent: "refresh",
      snapshot: expect.objectContaining({
        task: expect.objectContaining({ task_id: "task_1", title: "Updated adopted task" }),
      }),
    }));
  });

  it("loads fifteen additional visible tasks for each history-page request", () => {
    const requestNativeSessions = vi.fn();

    callbacks({ requestNativeSessions }).navigation.loadNativeSessions("cursor_2");

    expect(requestNativeSessions).toHaveBeenCalledWith("cursor_2", true, 15);
  });

  it("opens the prepared new Task while send capability is still preparing", async () => {
    vi.useFakeTimers();
    try {
      const dispatch = vi.fn();
      const request = vi.fn(async (method: string) => {
        if (method === TASK_CREATE) return { task: protocolPreparingTaskSnapshot("task_1", "New task") };
        if (method === TASK_OPEN) return { task: protocolTaskSnapshot("task_1", "New task") };
        if (method === TASK_SEND) return { task: { ...protocolTaskSnapshot("task_1", "New task"), revision: 3 } };
        throw new Error(method);
      });
      const state = createInitialState();
      state.newTask.prompt = "Build the thing";
      state.newTask.selection = {
        ...state.newTask.selection,
        agentId: "codex",
        agentLabel: "Codex",
        projectId: "project_1",
        workspaceRoot: "/workspace",
        workspaceLabel: "workspace",
      };

      callbacks({
        backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
        dispatch,
        state,
      }).newTask.submit();
      for (let index = 0; index < 8; index += 1) await Promise.resolve();

      expect(request).toHaveBeenNthCalledWith(1, TASK_CREATE, {
        projectId: "project_1",
        agentId: "codex",
      });
      expect(request).not.toHaveBeenCalledWith(TASK_OPEN, expect.anything());
      expect(request).toHaveBeenCalledWith(TASK_SEND, expect.objectContaining({
        taskId: "task_1",
        message: { text: "Build the thing" },
      }));
      expect(postHostMessage).toHaveBeenCalledWith({
        type: "surface.openTask",
        payload: {
          task_id: "task_1",
          title: "New task",
        },
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it("reopens and retries the first new-task send when preparation changes the task revision", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_CREATE) return { task: { ...protocolTaskSnapshot("task_1", "New task"), revision: 1 } };
      if (method === TASK_SEND && request.mock.calls.filter(([called]) => called === TASK_SEND).length === 1) {
        throw new AppServerProtocolError({
          error: {
            code: "conflict",
            message: "Task changed before the message was sent",
            recoverable: true,
          },
        });
      }
      if (method === TASK_OPEN) return { task: { ...protocolTaskSnapshot("task_1", "New task"), revision: 2 } };
      if (method === TASK_SEND) return { task: { ...protocolTaskSnapshot("task_1", "New task"), revision: 3 } };
      throw new Error(method);
    });
    const state = createInitialState();
    state.newTask.prompt = "Build the thing";
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project_1",
      workspaceRoot: "/workspace",
      workspaceLabel: "workspace",
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).newTask.submit();
    await settlePromises();

    expect(request).toHaveBeenNthCalledWith(2, TASK_SEND, {
      taskId: "task_1",
      taskRevision: 1,
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
      message: { text: "Build the thing" },
    });
    expect(request).toHaveBeenNthCalledWith(3, TASK_OPEN, { taskId: "task_1" });
    expect(request).toHaveBeenNthCalledWith(4, TASK_SEND, {
      taskId: "task_1",
      taskRevision: 2,
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
      message: { text: "Build the thing" },
    });
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({ type: "taskInput:error" }));
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openTask",
      payload: {
        task_id: "task_1",
        title: "New task",
      },
    });
  });

  it("clears new-task startup state when first send fails after preparation", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_CREATE) return { task: protocolTaskSnapshot("task_1", "New task") };
      if (method === TASK_SEND) throw new Error("send failed");
      throw new Error(method);
    });
    const state = createInitialState();
    state.newTask.prompt = "Build the thing";
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project_1",
      workspaceRoot: "/workspace",
      workspaceLabel: "workspace",
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).newTask.submit();
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({ type: "taskInput:prompt", taskId: "task_1", prompt: "Build the thing" });
    expect(dispatch).toHaveBeenCalledWith({ type: "taskInput:error", taskId: "task_1", message: "send failed" });
    expect(dispatch).toHaveBeenCalledWith({ type: "submit:error", message: "send failed" });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({
      type: "surface.openTask",
    }));
  });

  it("keeps pending new-task send recovery when navigation aborts the send request", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
    vi.stubGlobal("document", { visibilityState: "hidden" });
    try {
      const dispatch = vi.fn();
      const request = vi.fn(async (method: string) => {
        if (method === TASK_CREATE) return { task: protocolTaskSnapshot("task_1", "New task") };
        if (method === TASK_SEND) throw new Error("request aborted");
        throw new Error(method);
      });
      const state = createInitialState();
      state.newTask.prompt = "Build the thing";
      state.newTask.selection = {
        ...state.newTask.selection,
        agentId: "codex",
        agentLabel: "Codex",
        projectId: "project_1",
        workspaceRoot: "/workspace",
        workspaceLabel: "workspace",
      };

      callbacks({
        backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
        dispatch,
        state,
      }).newTask.submit();
      await settlePromises();

      expect(sessionStorage.getItem(PENDING_TASK_SEND_RECOVERY_KEY)).toContain("Build the thing");
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("sends immediately when the prepared Task already owns the selected config options", async () => {
    vi.useFakeTimers();
    try {
      const dispatch = vi.fn();
      const request = vi.fn(async (method: string) => {
        if (method === TASK_CREATE) {
          return { task: protocolPreparingTaskSnapshot("task_1", "New task") };
        }
        if (method === TASK_OPEN) {
          return { task: { ...protocolTaskSnapshot("task_1", "New task"), revision: 3 } };
        }
        if (method === TASK_SET_CONFIG_OPTION) {
          return { task: { ...protocolTaskSnapshot("task_1", "New task"), revision: 4 } };
        }
        if (method === TASK_SEND) {
          return { task: { ...protocolTaskSnapshot("task_1", "New task"), revision: 5 } };
        }
        throw new Error(method);
      });
      const state = createInitialState();
      state.newTask.prompt = "Build the thing";
      state.newTask.selection = {
        ...state.newTask.selection,
        agentId: "codex",
        agentLabel: "Codex",
        projectId: "project_1",
        configOptions: { mode: "agent" },
        workspaceRoot: "/workspace",
        workspaceLabel: "workspace",
      };

      callbacks({
        backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
        dispatch,
        state,
      }).newTask.submit();
      await Promise.resolve();
      await Promise.resolve();

      expect(request).toHaveBeenNthCalledWith(1, TASK_CREATE, {
        projectId: "project_1",
        agentId: "codex",
        configOptions: { mode: "agent" },
      });
      expect(request).not.toHaveBeenCalledWith(TASK_OPEN, expect.anything());
      expect(request).not.toHaveBeenCalledWith(TASK_SET_CONFIG_OPTION, {
        taskId: "task_1",
        configId: "mode",
        value: "agent",
        clientMutationId: expect.stringMatching(/^frontend-new-task-mode-/),
      });
      expect(request).toHaveBeenCalledWith(TASK_SEND, expect.objectContaining({
        taskId: "task_1",
      }));
    } finally {
      vi.useRealTimers();
    }
  });

  it("updates config options for an existing Task through the typed task request", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_SET_CONFIG_OPTION) return { task: { ...protocolTaskSnapshot("task_1", "Task"), revision: 2 } };
      throw new Error(method);
    });
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task.selectConfigOption("model", "gpt-5");
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SET_CONFIG_OPTION, {
      taskId: "task_1",
      configId: "model",
      value: "gpt-5",
      clientMutationId: expect.stringMatching(/^frontend-task-config-model-/),
    });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: "snapshot", intent: "refresh" }));
  });

  it("lets subscribed Task events own config option results", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_SET_CONFIG_OPTION) {
        return { task: { ...protocolTaskSnapshot("task_1", "Task"), revision: 2 } };
      }
      throw new Error(method);
    });
    const backendConnection = {
      events: vi.fn(),
      request: request as unknown as BackendConnection["request"],
      respond: vi.fn(),
    };
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({ backendConnection, dispatch, state }).task.selectConfigOption("model", "gpt-5");
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SET_CONFIG_OPTION, expect.objectContaining({
      taskId: "task_1",
      configId: "model",
      value: "gpt-5",
    }));
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({ type: "snapshot" }));
  });

  it("surfaces a new-task error when typed create prerequisites are missing", () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.newTask.prompt = "Build the thing";

    callbacks({ dispatch, state }).newTask.submit();

    expect(dispatch).toHaveBeenCalledWith({
      type: "submit:error",
      message: "App Server connection unavailable.",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("rejects raw new-task attachments without legacy create replay", () => {
    const dispatch = vi.fn();
    const request = vi.fn();
    const state = createInitialState();
    state.newTask.prompt = "Build the thing";
    state.newTask.context = [{ local_id: "ctx_1", kind: "file", label: "README.md", path: "/workspace/README.md" }];
    state.newTask.selection = {
      ...state.newTask.selection,
      projectId: "project_1",
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).newTask.submit();

    expect(request).not.toHaveBeenCalled();
    expect(dispatch).toHaveBeenCalledWith({
      type: "submit:error",
      message: "Reselect attachments from the file browser before sending.",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("updates settings preference optimistically before App Server confirmation", async () => {
    const dispatch = vi.fn();
    const setPreferences = vi.fn();
    const request = vi.fn(async () => ({
      preferences: { composerSubmitShortcut: "enter" },
    }));

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      setPreferences,
    }).settings.setComposerSubmitShortcut("enter");
    await settlePromises();

    const preferences = { composer_submit_shortcut: "enter" };
    expect(setPreferences).toHaveBeenNthCalledWith(1, preferences);
    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "settings:preferences", preferences });
    expect(request).toHaveBeenCalledWith(SETTINGS_UPDATE_PREFERENCES, {
      preferences: { composerSubmitShortcut: "enter" },
    });
    expect(setPreferences).toHaveBeenLastCalledWith(preferences);
    expect(dispatch).toHaveBeenLastCalledWith({ type: "settings:preferences", preferences });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("reports an error for preference updates when BackendConnection requests are unavailable", () => {
    const dispatch = vi.fn();
    const setPreferences = vi.fn();

    callbacks({ dispatch, setPreferences }).settings.setComposerSubmitShortcut("enter");

    expect(setPreferences).toHaveBeenCalledWith({ composer_submit_shortcut: "enter" });
    expect(dispatch).toHaveBeenCalledWith({
      type: "settings:error",
      message: "Agent catalog changes require the App Server.",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("updates ACP trace through BackendConnection runtime settings", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({
      developer: { acpTrace: { enabled: true, directory: "/runtime/traces" } },
    }));

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
    }).settings.setAcpTrace(true);
    await settlePromises();

    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "settings:developerAcpTrace", enabled: true });
    expect(request).toHaveBeenCalledWith(SETTINGS_UPDATE_RUNTIME, {
      developer: { acpTrace: { enabled: true } },
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "settings:runtimeSettings",
      settings: { developer: { acp_trace: { enabled: true, directory: "/runtime/traces" } } },
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("reports an error for ACP trace updates when BackendConnection requests are unavailable", () => {
    const dispatch = vi.fn();

    callbacks({ dispatch }).settings.setAcpTrace(true);

    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "settings:developerAcpTrace", enabled: true });
    expect(dispatch).toHaveBeenCalledWith({
      type: "settings:error",
      message: "Settings require the App Server.",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("creates custom Agents through BackendConnection when available", async () => {
    const dispatch = vi.fn();
    const setAgents = vi.fn();
    const request = vi.fn(async () => ({
      agentId: "custom.local",
      agents: protocolAgents(["codex", "custom.local"]),
    }));

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      setAgents,
    }).settings.createCustomAgent({
      label: "Local Agent",
      icon: "bot",
      command_line: "local-agent --stdio",
      enabled: true,
      env: [
        { name: "LOCAL_TOKEN", value: "secret-token", secret: true },
        { name: "LOG_LEVEL", value: "debug", secret: false },
      ],
    });
    await settlePromises();

    expect(request).toHaveBeenCalledWith(AGENT_CREATE_CUSTOM, {
      agentId: expect.stringMatching(/^custom\./),
      label: "Local Agent",
      icon: "bot",
      commandLine: "local-agent --stdio",
      command: "local-agent",
      args: ["--stdio"],
      env: { LOG_LEVEL: "debug" },
      secretEnv: ["LOCAL_TOKEN"],
      enabled: true,
    });
    expect(setAgents).toHaveBeenCalledWith(expect.arrayContaining([expect.objectContaining({ id: "custom.local" })]));
    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "settings:start" });
    expect(dispatch).toHaveBeenCalledWith({
      type: "settings:agentSaved",
      agentId: "custom.local",
      agent: expect.objectContaining({
        env: [
          { name: "LOCAL_TOKEN", secret: true },
          { name: "LOG_LEVEL", value: "debug", secret: false },
        ],
      }),
    });
    expect(JSON.stringify(dispatch.mock.calls)).not.toContain("secret-token");
    expect(beginAgentSecretTransaction).toHaveBeenCalledWith({
      writes: [
        {
          target: { kind: "agentEnvironment", agentId: expect.stringMatching(/^custom\./), name: "LOCAL_TOKEN" },
          value: "secret-token",
        },
      ],
      deletes: [],
    });
  });

  it("does not create a custom Agent when secure storage rejects its secrets", async () => {
    const dispatch = vi.fn();
    const request = vi.fn();
    beginAgentSecretTransaction.mockRejectedValueOnce(new Error("Secure storage is unavailable in the Web App."));

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
    }).settings.createCustomAgent({
      label: "Local Agent",
      icon: "bot",
      command_line: "local-agent --stdio",
      enabled: true,
      env: [{ name: "LOCAL_TOKEN", value: "secret-token", secret: true }],
    });
    await settlePromises();

    expect(request).not.toHaveBeenCalled();
    expect(dispatch).toHaveBeenCalledWith({
      type: "settings:error",
      message: "Secure storage is unavailable in the Web App.",
    });
    expect(JSON.stringify(dispatch.mock.calls)).not.toContain("secret-token");
  });

  it("refreshes Settings projections through BackendConnection when available", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === SETTINGS_GET_MCP_SERVERS) {
        return { generatedAt: "mcp-now", availability: "unavailable", servers: [] };
      }
      if (method === SETTINGS_GET_SKILLS) {
        return { generatedAt: "skills-now", availability: "unavailable", skills: [] };
      }
      return {
        generatedAt: "now",
        agents: [
          {
            agentId: "custom.local",
            label: "Local Agent",
            enabled: true,
            sourceKind: "custom",
            icon: "terminal",
            transport: "stdio",
            status: "connected",
            launchLabel: "local-agent",
            commandLine: "local-agent --stdio",
            env: [{ name: "LOCAL_TOKEN", value: "must-not-enter-state", secret: true }],
            description: "Custom ACP stdio Agent",
            capabilities: ["ACP stdio"],
            authMethods: [],
          },
        ],
      };
    });

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
    }).settings.refreshSettings();
    await settlePromises();

    expect(request).toHaveBeenCalledWith(SETTINGS_GET_AGENT_DETAILS, {});
    expect(request).toHaveBeenCalledWith(SETTINGS_GET_MCP_SERVERS, {});
    expect(request).toHaveBeenCalledWith(SETTINGS_GET_SKILLS, {});
    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "settings:start" });
    expect(dispatch).toHaveBeenCalledWith({ type: "settings:mcpServersStart" });
    expect(dispatch).toHaveBeenCalledWith({ type: "settings:skillsStart" });
    expect(dispatch).toHaveBeenCalledWith({
      type: "settings:agentDetailsResult",
      generatedAt: "now",
      agents: [expect.objectContaining({
        id: "custom.local",
        icon: "terminal",
        command_line: "local-agent --stdio",
        status: "connected",
      })],
    });
    expect(JSON.stringify(dispatch.mock.calls)).not.toContain("must-not-enter-state");
    expect(dispatch).toHaveBeenCalledWith({
      type: "settings:mcpServersResult",
      generatedAt: "mcp-now",
      availability: "unavailable",
      servers: [],
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "settings:skillsResult",
      generatedAt: "skills-now",
      availability: "unavailable",
      skills: [],
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("reports an error for Settings refresh when BackendConnection requests are unavailable", async () => {
    const dispatch = vi.fn();

    callbacks({ dispatch }).settings.refreshSettings();
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({
      type: "settings:error",
      message: "Settings require the App Server.",
    });
    expect(postHostMessage).not.toHaveBeenCalledWith({ type: "settings.snapshot" });
  });

  it("authenticates Agents through BackendConnection and refreshes Settings", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === AGENT_AUTHENTICATE) {
        return { agentId: "codex", methodId: "codex-login", status: "authenticated" };
      }
      if (method === SETTINGS_GET_AGENT_DETAILS) {
        return {
          generatedAt: "after-auth",
          agents: [
            {
              agentId: "codex",
              label: "Codex",
              enabled: true,
              sourceKind: "built_in",
              icon: "sparkles",
              transport: "stdio",
              status: "connected",
              launchLabel: "codex",
              description: "Codex ACP Agent",
              capabilities: ["ACP stdio"],
              authMethods: [],
            },
          ],
        };
      }
      throw new Error(method);
    });

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
    }).settings.authenticateAgent("codex", "codex-login");
    await settlePromises();

    expect(request).toHaveBeenNthCalledWith(1, AGENT_AUTHENTICATE, {
      agentId: "codex",
      methodId: "codex-login",
    });
    expect(request).toHaveBeenNthCalledWith(2, SETTINGS_GET_AGENT_DETAILS, {});
    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "settings:start" });
    expect(dispatch).toHaveBeenCalledWith({
      type: "settings:agentDetailsResult",
      generatedAt: "after-auth",
      agents: [expect.objectContaining({ id: "codex", status: "connected" })],
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("reports an error for Settings authentication when BackendConnection requests are unavailable", async () => {
    const dispatch = vi.fn();

    callbacks({ dispatch }).settings.authenticateAgent("codex", "codex-login");
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({
      type: "settings:error",
      message: "Agent catalog changes require the App Server.",
    });
    expect(postHostMessage).not.toHaveBeenCalledWith({
      type: "agent.authenticate",
      payload: { agent_id: "codex", method_id: "codex-login" },
    });
  });

  it("updates custom Agent metadata through BackendConnection when launch fields are unchanged", async () => {
    const request = vi.fn(async () => ({
      agentId: "custom.local",
      agents: protocolAgents(["codex", "custom.local"]),
    }));
    const state = createInitialState();
    state.settings.agentDetails = [customSettingsAgent("custom.local")];

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      state,
    }).settings.updateCustomAgentMetadata({
      agent_id: "custom.local",
      label: "Replacement Agent",
      icon: "bot",
      enabled: true,
    });
    await settlePromises();

    expect(request).toHaveBeenCalledWith(AGENT_UPDATE_CUSTOM_METADATA, expect.objectContaining({
      agentId: "custom.local",
      label: "Replacement Agent",
    }));
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("replaces custom Agent identity through BackendConnection for confirmed launch edits", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({
      oldAgentId: "custom.local",
      newAgentId: "custom.new",
      cleanup: {
        removedCatalogRecord: true,
        removedCachedStatus: false,
        removedSettingsOverlay: false,
        removedSecretEnv: ["TOKEN", "OLD_TOKEN"],
        historyPolicy: "preserveHistoricalTasks",
      },
      agents: protocolAgents(["codex", "custom.new"]),
    }));
    const state = createInitialState();
    state.settings.agentDetails = [customSettingsAgent("custom.local")];
    state.settings.agentDetails[0].env = [
      { name: "TOKEN", secret: true },
      { name: "OLD_TOKEN", secret: true },
    ];

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).settings.replaceCustomAgent({
      source_agent_id: "custom.local",
      label: "Replacement Agent",
      icon: "bot",
      command_line: "replacement-agent",
      enabled: true,
      env: [
        { name: "TOKEN", secret: true },
        { name: "ROTATED_TOKEN", value: "rotated-secret", secret: true },
      ],
      confirmed: true,
    });
    await settlePromises();

    expect(request).toHaveBeenCalledWith(AGENT_REPLACE_CUSTOM, expect.objectContaining({
      sourceAgentId: "custom.local",
      targetAgentId: expect.stringMatching(/^custom\./),
      expectedSourceSecretEnv: ["TOKEN", "OLD_TOKEN"],
      commandLine: "replacement-agent",
      confirmation: { acceptedLaunchIdentityChange: true },
    }));
    expect(beginAgentSecretTransaction).toHaveBeenCalledWith({
      writes: [
          {
            target: { kind: "agentEnvironment", agentId: expect.stringMatching(/^custom\./), name: "TOKEN" },
            copyFrom: { kind: "agentEnvironment", agentId: "custom.local", name: "TOKEN" },
          },
          {
            target: { kind: "agentEnvironment", agentId: expect.stringMatching(/^custom\./), name: "ROTATED_TOKEN" },
            value: "rotated-secret",
          },
      ],
      deletes: [
          { kind: "agentEnvironment", agentId: "custom.local", name: "TOKEN" },
          { kind: "agentEnvironment", agentId: "custom.local", name: "OLD_TOKEN" },
      ],
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "settings:agentReplaced",
      oldAgentId: "custom.local",
      newAgentId: "custom.new",
      agent: expect.objectContaining({
        env: [
          { name: "TOKEN", secret: true },
          { name: "ROTATED_TOKEN", secret: true },
        ],
      }),
    });
    expect(JSON.stringify(dispatch.mock.calls)).not.toContain("rotated-secret");
  });

  it("keeps App Server-owned Agent mutations local when BackendConnection requests are unavailable", async () => {
    const dispatch = vi.fn();
    const settings = callbacks().settings;

    callbacks({ dispatch }).settings.createCustomAgent({
      label: "Local Agent",
      icon: "bot",
      command_line: "local-agent",
      enabled: true,
      env: [],
    });
    settings.deleteCustomAgent("custom.local");
    settings.setAgentEnabled("codex", false);
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({ type: "settings:error", message: "Agent catalog changes require the App Server." });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "agent.custom.delete" }));
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "agent.enabled.set" }));
  });

  it("deletes custom Agents through BackendConnection when available", async () => {
    const dispatch = vi.fn();
    const setAgents = vi.fn();
    const request = vi.fn(async () => ({
      agentId: "custom.local",
      removedSecretEnv: ["TOKEN"],
      agents: protocolAgents(["codex"]),
    }));
    const state = createInitialState();
    state.settings.agentDetails = [customSettingsAgent("custom.local")];
    state.settings.agentDetails[0].env = [{ name: "TOKEN", secret: true }];

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      setAgents,
      state,
    }).settings.deleteCustomAgent("custom.local");
    await settlePromises();

    expect(request).toHaveBeenCalledWith(AGENT_DELETE_CUSTOM, {
      agentId: "custom.local",
      expectedSecretEnv: ["TOKEN"],
    });
    expect(setAgents).toHaveBeenCalledWith([expect.objectContaining({ id: "codex" })]);
    expect(dispatch).toHaveBeenCalledWith({ type: "settings:agentDeleted", agentId: "custom.local" });
    expect(beginAgentSecretTransaction).toHaveBeenCalledWith({
      writes: [],
      deletes: [
        { kind: "agentEnvironment", agentId: "custom.local", name: "TOKEN" },
      ],
    });
  });

  it("sets Agent availability through BackendConnection when available", async () => {
    const dispatch = vi.fn();
    const setAgents = vi.fn();
    const request = vi.fn(async () => ({ agents: protocolAgents(["codex"]) }));
    const state = createInitialState();

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      setAgents,
      state,
    }).settings.setAgentEnabled("codex", false);
    await settlePromises();

    expect(request).toHaveBeenCalledWith(AGENT_SET_ENABLED, { agentId: "codex", enabled: false });
    expect(setAgents).toHaveBeenCalledWith([expect.objectContaining({ id: "codex" })]);
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "settings:agentUpdated",
      agent: expect.objectContaining({ id: "codex", enabled: false, status: "disabled" }),
    }));
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("keeps settings errors local after BackendConnection rejection", async () => {
    const dispatch = vi.fn();

    callbacks({
      backendConnection: {
        request: vi.fn(async () => { throw new Error("duplicate Agent"); }) as unknown as BackendConnection["request"],
        respond: vi.fn(),
      },
      dispatch,
    }).settings.createCustomAgent({
      label: "Local Agent",
      icon: "bot",
      command_line: "local-agent",
      enabled: true,
      env: [],
    });
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({ type: "settings:error", message: "duplicate Agent" });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("surfaces an error when sending without a BackendConnection", () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.taskInputs.task_1 = { prompt: "Continue", context: [] };

    callbacks({ dispatch, state }).task.sendPrompt();

    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:error",
      taskId: "task_1",
      message: "App Server connection unavailable.",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("sends text-only task prompts through BackendConnection when available", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({ task: protocolTaskSnapshot("task_1", "Sent") }));
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.taskInputs.task_1 = { prompt: "Continue", context: [] };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task.sendPrompt();
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SEND, {
      taskId: "task_1",
      taskRevision: 1,
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
      message: { text: "Continue" },
    });
    expect(dispatch).toHaveBeenNthCalledWith(1, {
      type: "taskInput:submit",
      taskId: "task_1",
      input: { prompt: "Continue", context: [] },
    });
    expect(dispatch).toHaveBeenNthCalledWith(2, expect.objectContaining({ type: "snapshot", intent: "refresh" }));
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("sends task prompts through BackendConnection while the task is active", () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => {
      const task = protocolTaskSnapshot("task_1", "Task");
      return { task: { ...task, task: { ...task.task, status: "running" as const } } };
    });
    const state = createInitialState();
    state.snapshot = {
      ...snapshot("task_1"),
      task: { ...snapshot("task_1").task, status: "active" },
    };
    state.taskInputs.task_1 = { prompt: "Follow up next", context: [] };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task.sendPrompt();

    expect(request).toHaveBeenCalledWith(TASK_SEND, expect.objectContaining({
      taskId: "task_1",
      message: { text: "Follow up next" },
    }));
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:submit",
      taskId: "task_1",
      input: { prompt: "Follow up next", context: [] },
    }));
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("sends App Server handle-backed attachments through BackendConnection", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({ task: protocolTaskSnapshot("task_1", "Sent") }));
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.taskInputs.task_1 = {
      prompt: "Continue",
      context: [
        {
          local_id: "ctx_1",
          kind: "file",
          label: "notes.md",
          app_server_handle_id: "attachment-handle-1" as AttachmentHandleId,
        },
      ],
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task.sendPrompt();
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SEND, {
      taskId: "task_1",
      taskRevision: 1,
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
      message: {
        text: "Continue",
        attachments: ["attachment-handle-1"],
      },
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("rejects raw attachment prompt sends without legacy replay", () => {
    const dispatch = vi.fn();
    const request = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.taskInputs.task_1 = {
      prompt: "Continue",
      context: [{ local_id: "ctx_1", kind: "context", label: "workspace", path: "/workspace" }],
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task.sendPrompt();

    expect(request).not.toHaveBeenCalled();
    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:error",
      taskId: "task_1",
      message: "Reselect attachments from the file browser before sending.",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("releases App Server attachment handles when task composer attachments are removed", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({ releasedHandles: ["attachment-handle-1"] }));
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.taskInputs.task_1 = {
      prompt: "Continue",
      context: [
        {
          local_id: "ctx_1",
          kind: "file",
          label: "notes.md",
          app_server_handle_id: "attachment-handle-1" as AttachmentHandleId,
        },
      ],
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task.removeAttachment("ctx_1");
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:attachment:remove",
      taskId: "task_1",
      attachmentId: "ctx_1",
    });
    expect(request).toHaveBeenCalledWith(ATTACHMENT_RELEASE_HANDLES, {
      taskId: "task_1",
      handles: ["attachment-handle-1"],
    });
  });

  it("reveals App Server attachment handles through BackendConnection", async () => {
    const request = vi.fn(async () => ({ requested: true }));
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.taskInputs.task_1 = {
      prompt: "Continue",
      context: [
        {
          local_id: "ctx_1",
          kind: "file",
          label: "notes.md",
          app_server_handle_id: "attachment-handle-1" as AttachmentHandleId,
        },
      ],
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      state,
    }).task.revealAttachment("ctx_1");
    await settlePromises();

    expect(request).toHaveBeenCalledWith(ATTACHMENT_REVEAL, {
      taskId: "task_1",
      handleId: "attachment-handle-1",
    });
  });

  it("loads App Server file browser entries for the active task", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === ATTACHMENT_LIST_ROOTS) return { roots: [{ rootId: "root-1", label: "Workspace" }] };
      if (method === ATTACHMENT_LIST_DIRECTORY) {
        return {
          directory: { rootId: "root-1", label: "Workspace" },
          entries: [{ entryId: "entry-1", kind: "file", label: "notes.md", selectable: true }],
        };
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    const task = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      state,
    }).task;

    await expect(task.fileBrowser?.listRoots()).resolves.toEqual([{ rootId: "root-1", label: "Workspace" }]);
    await expect(task.fileBrowser?.listDirectory("root-1" as FileBrowserRootId)).resolves.toEqual({
      directory: { rootId: "root-1", label: "Workspace" },
      entries: [{ entryId: "entry-1", kind: "file", label: "notes.md", selectable: true }],
    });

    expect(request).toHaveBeenCalledWith(ATTACHMENT_LIST_ROOTS, { taskId: "task_1" });
    expect(request).toHaveBeenCalledWith(ATTACHMENT_LIST_DIRECTORY, {
      taskId: "task_1",
      rootId: "root-1",
      directoryId: undefined,
    });
  });

  it("adds App Server file reference attachments from the file browser", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({
      attachment: { handleId: "attachment-handle-1", label: "notes.md" },
    }));
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    await callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task.fileBrowser?.attachFileReference("entry-1" as FileBrowserEntryId);

    expect(request).toHaveBeenCalledWith(ATTACHMENT_CREATE_FILE_REFERENCE, {
      taskId: "task_1",
      entryId: "entry-1",
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:attachment:addAppServer",
      taskId: "task_1",
      attachment: expect.objectContaining({
        kind: "file",
        label: "notes.md",
        app_server_handle_id: "attachment-handle-1",
      }),
    });
  });

  it("confirms embedded candidates before adding App Server embedded attachments", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === ATTACHMENT_CREATE_EMBEDDED_CANDIDATE) {
        return { candidate: { candidateId: "candidate-1", label: "notes.md" } };
      }
      if (method === ATTACHMENT_CONFIRM_EMBEDDED) {
        return {
          attachments: [{ handleId: "attachment-handle-2", label: "notes.md" }],
          errors: [],
        };
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    await callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task.fileBrowser?.attachEmbedded("entry-1" as FileBrowserEntryId);

    expect(request).toHaveBeenCalledWith(ATTACHMENT_CREATE_EMBEDDED_CANDIDATE, {
      taskId: "task_1",
      entryId: "entry-1",
    });
    expect(request).toHaveBeenCalledWith(ATTACHMENT_CONFIRM_EMBEDDED, {
      taskId: "task_1",
      candidates: ["candidate-1"],
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:attachment:addAppServer",
      taskId: "task_1",
      attachment: expect.objectContaining({
        label: "notes.md",
        app_server_handle_id: "attachment-handle-2",
      }),
    });
  });

  it("adds pasted image attachments through BackendConnection", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({
      attachment: { handleId: "attachment-handle-image", label: "pasted.png" },
    }));
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    await callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task.fileBrowser?.attachPastedImage(new File(["image"], "pasted.png", { type: "image/png" }));

    expect(request).toHaveBeenCalledWith(ATTACHMENT_CREATE_PASTED_IMAGE, {
      taskId: "task_1",
      label: "pasted.png",
      mimeType: "image/png",
      data: "aW1hZ2U=",
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:attachment:addAppServer",
      taskId: "task_1",
      attachment: expect.objectContaining({
        kind: "file",
        label: "pasted.png",
        app_server_handle_id: "attachment-handle-image",
        preview_url: "data:image/png;base64,aW1hZ2U=",
      }),
    });
  });

  it("cancels tasks through BackendConnection when available", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({ task: protocolTaskSnapshot("task_1", "Cancelled") }));
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task.cancel();
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_CANCEL, { taskId: "task_1" });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: "snapshot", intent: "refresh" }));
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("surfaces an error when cancel has no BackendConnection", () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({ dispatch, state }).task.cancel();

    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:error",
      taskId: "task_1",
      message: "App Server connection unavailable.",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("restores typed prompt sends after request rejection", async () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.taskInputs.task_1 = { prompt: "Continue", context: [] };

    callbacks({
      backendConnection: {
        request: vi.fn(async () => { throw new Error("stale revision"); }) as unknown as BackendConnection["request"],
        respond: vi.fn(),
      },
      dispatch,
      state,
    }).task.sendPrompt();
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:error",
      taskId: "task_1",
      message: "stale revision",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("restores typed prompt sends after authoritative protocol rejection", async () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.taskInputs.task_1 = { prompt: "Continue", context: [] };

    callbacks({
      backendConnection: {
        request: vi.fn(async () => {
          throw new AppServerProtocolError({
            error: { code: "conflict", message: "Task is running", recoverable: true },
          });
        }) as unknown as BackendConnection["request"],
        respond: vi.fn(),
      },
      dispatch,
      state,
    }).task.sendPrompt();
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:error",
      taskId: "task_1",
      message: "Task is running",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("surfaces typed cancel request failures without legacy replay", async () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({
      backendConnection: {
        request: vi.fn(async () => { throw new Error("not running"); }) as unknown as BackendConnection["request"],
        respond: vi.fn(),
      },
      dispatch,
      state,
    }).task.cancel();
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:error",
      taskId: "task_1",
      message: "not running",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("rejects legacy Agent permission responses without bridge replay", () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({ dispatch, state }).task.respondToPermission("permission_1", "allow_once", "approved");

    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "permission:responding", requestId: "permission_1" });
    expect(dispatch).toHaveBeenNthCalledWith(2, {
      type: "permission:error",
      requestId: "permission_1",
      message: "Permission request is no longer answerable.",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("answers permissions through BackendConnection when available", async () => {
    const dispatch = vi.fn();
    const respond = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({ backendConnection: { respond }, dispatch, state }).task.respondToPermission(
      "server-request-1",
      "allow_once",
      "approved",
      "appServer",
    );

    expect(dispatch).toHaveBeenNthCalledWith(1, {
      type: "permission:responding",
      requestId: "server-request-1",
    });
    expect(respond).toHaveBeenCalledWith("server-request-1", { optionId: "allow_once" });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("clears App Server permission state after an accepted response", async () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({
      backendConnection: { respond: vi.fn(async () => undefined) },
      dispatch,
      state,
    }).task.respondToPermission("server-request-1", "allow_once", "approved", "appServer");
    await settlePromises();

    expect(dispatch).toHaveBeenNthCalledWith(1, {
      type: "permission:responding",
      requestId: "server-request-1",
    });
    expect(dispatch).toHaveBeenNthCalledWith(2, {
      type: "appServerPermission:resolved",
      requestId: "server-request-1",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("does not answer server-shaped legacy Agent permission ids through BackendConnection", () => {
    const dispatch = vi.fn();
    const respond = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({ backendConnection: { respond }, dispatch, state }).task.respondToPermission(
      "server-request-shaped-agent-id",
      "allow_once",
      "approved",
    );

    expect(respond).not.toHaveBeenCalled();
    expect(dispatch).toHaveBeenNthCalledWith(2, {
      type: "permission:error",
      requestId: "server-request-shaped-agent-id",
      message: "Permission request is no longer answerable.",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("shows a recoverable permission error when BackendConnection respond fails", async () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({
      backendConnection: { respond: () => Promise.reject(new Error("connection closed")) },
      dispatch,
      state,
    }).task.respondToPermission("server-request-1", "allow_once", "approved", "appServer");
    await Promise.resolve();

    expect(dispatch).toHaveBeenNthCalledWith(1, {
      type: "permission:responding",
      requestId: "server-request-1",
    });
    expect(dispatch).toHaveBeenNthCalledWith(2, {
      type: "permission:error",
      requestId: "server-request-1",
      message: "connection closed",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("shows a recoverable permission error when App Server source has no connection", () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({ dispatch, state }).task.respondToPermission(
      "server-request-1",
      "allow_once",
      "approved",
      "appServer",
    );

    expect(dispatch).toHaveBeenNthCalledWith(1, {
      type: "permission:responding",
      requestId: "server-request-1",
    });
    expect(dispatch).toHaveBeenNthCalledWith(2, {
      type: "permission:error",
      requestId: "server-request-1",
      message: "App Server connection unavailable",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("shows a recoverable permission error when BackendConnection respond throws", () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({
      backendConnection: {
        respond: () => {
          throw new Error("connection unavailable");
        },
      },
      dispatch,
      state,
    }).task.respondToPermission("server-request-1", "allow_once", "approved", "appServer");

    expect(dispatch).toHaveBeenNthCalledWith(1, {
      type: "permission:responding",
      requestId: "server-request-1",
    });
    expect(dispatch).toHaveBeenNthCalledWith(2, {
      type: "permission:error",
      requestId: "server-request-1",
      message: "connection unavailable",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("rejects external native session adoption without BackendConnection", () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      workspaceRoot: "/workspace",
    };

    callbacks({ dispatch, state }).navigation.openNativeSession({
      cwd: "/workspace",
      session_id: "native_1",
      title: "Native Session",
      updated_at: "2026-06-27T00:00:00Z",
    });

    expect(dispatch).toHaveBeenNthCalledWith(1, {
      type: "newTask:nativeSessions:adopt",
      sessionId: "native_1",
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:nativeSessions:error",
      message: "App Server connection unavailable.",
    });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "task.create" }));
  });

  it("opens an external native session through BackendConnection when available", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({ task: protocolTaskSnapshot("task_1", "Native Session") }));
    const state = createInitialState();
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project_1",
      workspaceRoot: "/workspace",
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).navigation.openNativeSession({
      cwd: "/workspace",
      session_id: "native_1",
      title: "Native Session",
      updated_at: "2026-06-27T00:00:00Z",
    });
    await settlePromises();

    expect(dispatch).toHaveBeenNthCalledWith(1, {
      type: "newTask:nativeSessions:adopt",
      sessionId: "native_1",
    });
    expect(request).toHaveBeenCalledWith(TASK_ADOPT_NATIVE_SESSION, {
      projectId: "project_1",
      agentId: "codex",
      nativeSessionId: "native_1",
      title: "Native Session",
    });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "snapshot",
      intent: "open",
    }));
    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:nativeSessions:remove",
      sessionId: "native_1",
    });
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openTask",
      payload: { task_id: "task_1", title: "Native Session" },
    });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "task.create" }));
  });

  it("does not redirect when native-session adoption resolves after a newer navigation intent", async () => {
    const dispatch = vi.fn();
    let resolveRequest: ((value: { task: ReturnType<typeof protocolTaskSnapshot> }) => void) | undefined;
    const request = vi.fn(() => new Promise<{ task: ReturnType<typeof protocolTaskSnapshot> }>((resolve) => {
      resolveRequest = resolve;
    }));
    let generation = 0;
    const beginNavigationChange = vi.fn(() => {
      generation += 1;
      return generation;
    });
    const currentNavigationGeneration = vi.fn(() => generation);
    const state = createInitialState();
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project_1",
      workspaceRoot: "/workspace",
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      beginNavigationChange,
      currentNavigationGeneration,
      dispatch,
      state,
    }).navigation.openNativeSession({
      cwd: "/workspace",
      session_id: "native_1",
      title: "Native Session",
      updated_at: "2026-06-27T00:00:00Z",
    });

    generation += 1;
    resolveRequest?.({ task: protocolTaskSnapshot("task_1", "Native Session") });
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:nativeSessions:adopt",
      sessionId: "native_1",
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:nativeSessions:remove",
      sessionId: "native_1",
    });
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({
      type: "snapshot",
      intent: "open",
    }));
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "surface.openTask" }));
  });

  it("does not surface a superseded native-session adoption error", async () => {
    const dispatch = vi.fn();
    let rejectRequest: ((error: Error) => void) | undefined;
    const request = vi.fn(() => new Promise((_, reject) => {
      rejectRequest = reject;
    }));
    let generation = 0;
    const state = createInitialState();
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project_1",
      workspaceRoot: "/workspace",
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      beginNavigationChange: () => ++generation,
      currentNavigationGeneration: () => generation,
      dispatch,
      state,
    }).navigation.openNativeSession({
      cwd: "/workspace",
      session_id: "native_1",
      title: "Native Session",
      updated_at: "2026-06-27T00:00:00Z",
    });

    generation += 1;
    rejectRequest?.(new Error("Slow session load failed"));
    await settlePromises();

    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({
      type: "newTask:nativeSessions:error",
    }));
  });

  it("clears active selection and reports an error when archiving without BackendConnection", () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.activeTaskId = "task_1";
    state.showArchived = true;

    callbacks({ dispatch, state }).navigation.archiveTask("task_1");

    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "selection:clear" });
    expect(dispatch).toHaveBeenCalledWith({
      type: "tasks:error",
      message: "App Server connection unavailable.",
    });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "task.archive" }));
  });

  it("archives and restores tasks through BackendConnection when available", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_SET_ARCHIVED) {
        return {
          taskId: "task_1",
          archived: true,
          tasks: { tasks: [protocolTaskSummary("task_2", "Remaining")], activeTaskId: null },
        };
      }
      if (method === TASK_LIST) {
        return { revision: 2, tasks: [protocolTaskSummary("task_2", "Remaining")] };
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.activeTaskId = "task_1";
    state.tasks = [{
      agent_id: "codex",
      agent_name: "Codex",
      created_at: "2026-05-22T00:00:00.000Z",
      has_messages: true,
      isolation: "local",
      last_activity: "2026-05-22T00:00:00.000Z",
      message_history_version: 1,
      project_id: "project_1",
      project_label: "OpenAIDE",
      status: "inactive",
      task_id: "task_1",
      task_version: 1,
      title: "Archived task",
      unread: false,
      updated_at: "2026-05-22T00:00:00.000Z",
      workspace_root: "/workspace",
    }];

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).navigation.archiveTask("task_1");
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SET_ARCHIVED, { taskId: "task_1", archived: true });
    expect(request).toHaveBeenCalledWith(TASK_LIST, { archived: false });
    expect(dispatch).toHaveBeenCalledWith({ type: "selection:clear" });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "tasks",
      tasks: [expect.objectContaining({ task_id: "task_2" })],
    }));
    expect(dispatch).toHaveBeenCalledWith({ type: "task:list:remove", taskId: "task_1" });
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openNewTask",
      payload: { project_id: "project_1" },
    });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "task.archive" }));

    dispatch.mockClear();
    request.mockClear();
    request.mockImplementation(async (method: string) => {
      if (method === TASK_SET_ARCHIVED) {
        return {
          taskId: "task_1",
          archived: false,
          tasks: { tasks: [protocolTaskSummary("task_1", "Restored")], activeTaskId: null },
        };
      }
      if (method === TASK_LIST) {
        return { revision: 3, tasks: [protocolTaskSummary("task_1", "Restored")] };
      }
      throw new Error(method);
    });
    state.showArchived = true;
    state.activeTaskId = undefined;
    state.snapshot = undefined;
    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).navigation.restoreTask("task_1");
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SET_ARCHIVED, { taskId: "task_1", archived: false });
    expect(request).toHaveBeenCalledWith(TASK_LIST, { archived: false });
    expect(dispatch).toHaveBeenCalledWith({ type: "archive:set", showArchived: false });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "tasks",
      tasks: [expect.objectContaining({ task_id: "task_1" })],
    }));
    expect(dispatch).not.toHaveBeenCalledWith({ type: "task:list:remove", taskId: "task_1" });
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openTask",
      payload: { task_id: "task_1" },
    });
  });

  it("restores the visible archived task into normal task context without preserving local draft", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_SET_ARCHIVED) {
        return {
          taskId: "task_1",
          archived: false,
          tasks: { tasks: [protocolTaskSummary("task_1", "Restored")], activeTaskId: null },
        };
      }
      if (method === TASK_LIST) {
        return { revision: 3, tasks: [protocolTaskSummary("task_1", "Restored")] };
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.showArchived = true;
    state.activeTaskId = "task_1";
    state.snapshot = snapshot("task_1");
    state.taskInputs.task_1 = { prompt: "testing archived composer", context: [] };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      beginNavigationChange: vi.fn(),
      dispatch,
      state,
    }).navigation.restoreTask("task_1");
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SET_ARCHIVED, { taskId: "task_1", archived: false });
    expect(request).toHaveBeenCalledWith(TASK_LIST, { archived: false });
    expect(dispatch).toHaveBeenCalledWith({ type: "taskInput:clear", taskId: "task_1" });
    expect(dispatch).toHaveBeenCalledWith({ type: "archive:set", showArchived: false });
    expect(dispatch).not.toHaveBeenCalledWith({ type: "task:list:remove", taskId: "task_1" });
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openTask",
      payload: { task_id: "task_1" },
    });
  });

  it("leaves the archived task route when the current snapshot is active but selection is not", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_SET_ARCHIVED) {
        return {
          taskId: "task_1",
          archived: true,
          tasks: { tasks: [protocolTaskSummary("task_2", "Remaining")], activeTaskId: null },
        };
      }
      if (method === TASK_LIST) {
        return { revision: 2, tasks: [protocolTaskSummary("task_2", "Remaining")] };
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.snapshot.task.project_id = "project_1";
    state.tasks = [state.snapshot.task];

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).navigation.archiveTask("task_1");
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({ type: "selection:clear" });
    expect(dispatch).toHaveBeenCalledWith({ type: "task:list:remove", taskId: "task_1" });
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openNewTask",
      payload: { project_id: "project_1" },
    });
  });

  it("opens the new-task route immediately when archiving the visible task", () => {
    const dispatch = vi.fn();
    const pendingArchive = deferred<unknown>();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_SET_ARCHIVED) return pendingArchive.promise;
      throw new Error(method);
    });
    const state = createInitialState();
    state.activeTaskId = "task_1";
    state.tasks = [{
      agent_id: "codex",
      agent_name: "Codex",
      created_at: "2026-05-22T00:00:00.000Z",
      has_messages: true,
      isolation: "local",
      last_activity: "2026-05-22T00:00:00.000Z",
      message_history_version: 1,
      project_id: "project_1",
      project_label: "OpenAIDE",
      status: "inactive",
      task_id: "task_1",
      task_version: 1,
      title: "Archived task",
      unread: false,
      updated_at: "2026-05-22T00:00:00.000Z",
      workspace_root: "/workspace",
    }];

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).navigation.archiveTask("task_1");

    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openNewTask",
      payload: { project_id: "project_1" },
    });
  });

  it("toggles archive mode and reports an error when listing without BackendConnection", () => {
    const beginNavigationChange = vi.fn();
    const dispatch = vi.fn();
    const state = createInitialState();

    callbacks({ beginNavigationChange, dispatch, state }).navigation.toggleArchived();

    expect(beginNavigationChange).toHaveBeenCalledWith(true);
    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "archive:set", showArchived: true });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "surface.openArchive" }));
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "surface.openNewTask" }));
    expect(dispatch).toHaveBeenCalledWith({
      type: "tasks:error",
      message: "App Server connection unavailable.",
    });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "task.list" }));
    expectCalledBefore(beginNavigationChange, dispatch);
  });

  it("toggles archive mode through typed task list when BackendConnection is available", async () => {
    const beginNavigationChange = vi.fn();
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({
      revision: 7,
      tasks: [protocolTaskSummary("task_archived", "Archived")],
      nextCursor: null,
    }));
    const state = createInitialState();

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      beginNavigationChange,
      dispatch,
      state,
    }).navigation.toggleArchived();
    await settlePromises();

    expect(beginNavigationChange).toHaveBeenCalledWith(true);
    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "archive:set", showArchived: true });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "surface.openArchive" }));
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "surface.openNewTask" }));
    expect(request).toHaveBeenCalledWith(TASK_LIST, { archived: true });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "tasks",
      tasks: [expect.objectContaining({ task_id: "task_archived" })],
    }));
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "task.list" }));
  });

  it("uses cached archive task list without requesting it again", async () => {
    const beginNavigationChange = vi.fn();
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({
      revision: 7,
      tasks: [protocolTaskSummary("task_archived_fresh", "Fresh Archived")],
    }));
    const state = createInitialState();
    state.taskListCache.archived = [snapshot("task_archived_cached").task];

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      beginNavigationChange,
      dispatch,
      state,
    }).navigation.toggleArchived();
    await settlePromises();

    expect(beginNavigationChange).toHaveBeenCalledWith(true);
    expect(dispatch).toHaveBeenCalledWith({ type: "archive:set", showArchived: true });
    expect(request).not.toHaveBeenCalled();
  });

  it("reports an error for config option changes without BackendConnection", () => {
    const dispatch = vi.fn();
    const latestOptionsRequestKey = { current: undefined as string | undefined };
    const state = createInitialState();
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      projectId: "project-1",
    };

    callbacks({ dispatch, latestOptionsRequestKey, state }).newTask.selectConfigOption("model", "gpt-5");

    expect(latestOptionsRequestKey.current).toBeUndefined();
    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:configOptions:error",
      message: "Task session is not ready yet.",
    });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "session.setConfigOption" }));
  });

  it("uses the prepared Task Native Session for config option changes", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({ task: protocolTaskSnapshot("task-prepared", "New task") }));
    const latestOptionsRequestKey = { current: undefined as string | undefined };
    const state = createInitialState();
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      projectId: "project-1",
      workspaceRoot: "/workspace/app",
    };
    state.snapshot = snapshot("task-prepared");
    state.snapshot.task.has_messages = false;
    state.snapshot.chat.has_messages = false;

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      latestOptionsRequestKey,
      state,
    }).newTask.selectConfigOption("model", "gpt-5");
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SET_CONFIG_OPTION, {
      taskId: "task-prepared",
      configId: "model",
      value: "gpt-5",
      clientMutationId: expect.any(String),
    });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: "snapshot", intent: "refresh" }));
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "session.setConfigOption" }));
  });

  it("does not reload tool details that are already loading or loaded", () => {
    const dispatch = vi.fn();
    const loadingState = createInitialState();
    loadingState.snapshot = snapshot("task_1");
    loadingState.toolDetails[toolDetailCacheKey("task_1", "artifact_1")] = { loading: true };

    callbacks({ dispatch, state: loadingState }).task.loadToolDetail("artifact_1");

    expect(dispatch).not.toHaveBeenCalled();
    expect(postHostMessage).not.toHaveBeenCalled();

    const loadedState = createInitialState();
    loadedState.snapshot = snapshot("task_1");
    loadedState.toolDetails[toolDetailCacheKey("task_1", "artifact_1")] = {
      details: { content: [], input: undefined, locations: [] },
      loading: false,
    };

    callbacks({ dispatch, state: loadedState }).task.loadToolDetail("artifact_1");

    expect(dispatch).not.toHaveBeenCalled();
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("loads earlier chat pages through BackendConnection", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({
      taskId: "task_1",
      items: [{
        messageId: "msg_1",
        role: "agent",
        status: "complete",
        parts: [{ kind: "text", text: "hello" }],
      }],
      hasBefore: false,
      totalCount: 1,
      revision: 2,
      startCursor: "msg_1",
      endCursor: "msg_1",
    }));
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task.loadChatPage("cursor_1");
    await settlePromises();

    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "chatPage:start", taskId: "task_1" });
    expect(request).toHaveBeenCalledWith(TASK_CHAT_PAGE, {
      taskId: "task_1",
      beforeCursor: "cursor_1",
      limit: 50,
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "chatPage:result",
      taskId: "task_1",
      page: expect.objectContaining({
        task_id: "task_1",
        items: [expect.objectContaining({ message_id: "msg_1" })],
      }),
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("loads tool details through BackendConnection", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({
      locations: [{ path: "src/main.rs", line: 7 }],
      content: [{ kind: "text", text: "details" }],
      input: null,
      output: { exitCode: 0, success: true, fields: [] },
    }));
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task.loadToolDetail("artifact_1");
    await settlePromises();

    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "toolDetail:start", taskId: "task_1", artifactId: "artifact_1" });
    expect(request).toHaveBeenCalledWith(TASK_TOOL_DETAIL, {
      taskId: "task_1",
      artifactId: "artifact_1",
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "toolDetail:result",
      taskId: "task_1",
      artifactId: "artifact_1",
      details: expect.objectContaining({
        locations: [{ path: "src/main.rs", line: 7 }],
        output: expect.objectContaining({ exit_code: 0 }),
      }),
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("task callbacks no-op when there is no active snapshot", () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    const task = callbacks({ dispatch, state }).task;

    task.cancel();
    task.loadChatPage("cursor_1");
    task.loadToolDetail("artifact_1");
    task.respondToPermission("permission_1", "allow_once", "approved");
    task.sendPrompt();

    expect(dispatch).not.toHaveBeenCalled();
    expect(postHostMessage).not.toHaveBeenCalled();
  });

});

function callbacks({
  backendConnection,
  beginNavigationChange = vi.fn(() => 1),
  currentNavigationGeneration = vi.fn(() => 1),
  dispatch = vi.fn(),
  latestOptionsRequestKey = { current: undefined as string | undefined },
  newTaskStartAttempt = { current: undefined },
  pendingPreparedNewTask = vi.fn(() => undefined),
  requestNativeSessions = vi.fn(),
  setAgents = vi.fn(),
  setPreferences = vi.fn(),
  state = createInitialState(),
}: Partial<Parameters<typeof createAppCallbacks>[0]> = {}) {
  return createAppCallbacks({
    backendConnection,
    beginNavigationChange,
    createSnapshotRequestId: vi.fn(() => 91),
    currentNavigationGeneration,
    dispatch,
    latestOptionsRequestKey,
    newTaskStartAttempt,
    pendingPreparedNewTask,
    requestNativeSessions,
    setAgents,
    setPreferences,
    state,
  });
}

function expectCalledBefore(first: ReturnType<typeof vi.fn>, second: ReturnType<typeof vi.fn>) {
  expect(first.mock.invocationCallOrder[0]).toBeLessThan(second.mock.invocationCallOrder[0]);
}

function settlePromises() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function snapshot(taskId: string): TaskSnapshot {
  return {
    task: {
      task_id: taskId,
      title: "Task",
      status: "inactive",
      task_version: 1,
      message_history_version: 1,
      has_messages: true,
      unread: false,
      created_at: "2026-05-22T00:00:00.000Z",
      updated_at: "2026-05-22T00:00:00.000Z",
      last_activity: "2026-05-22T00:00:00.000Z",
      agent_id: "codex",
      agent_name: "Codex",
      isolation: "local",
      workspace_root: "/workspace",
    },
    history_sync: { state: "idle", generation: 0 },
    chat: {
      task_id: taskId,
      items: [],
      has_before: false,
      has_messages: true,
      total_count: 0,
      version: 1,
    },
    permissions: [],
    send_capability: { state: "ready", attachment_only: true },
    settings_summary: {
      agent_id: "codex",
      isolation: "local",
      config_options: {},
    },
    revision: 1,
  };
}

function protocolTaskSnapshot(taskId: string, title: string): ProtocolTaskSnapshot {
  return {
    task: protocolTaskSummary(taskId, title),
    revision: 2,
    preparation: { kind: "ready" as const },
    agentConfig: { state: "ready" as const, options: [] },
    agentCommands: { state: "ready" as const, commands: [] },
    sendCapability: { state: "ready" as const },
    historySync: { state: "idle", generation: 0 },
    chat: { items: [], hasMoreBefore: false, hasMessages: true },
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function memoryStorage(): Storage {
  const items = new Map<string, string>();
  return {
    get length() {
      return items.size;
    },
    clear: () => items.clear(),
    getItem: (key) => items.get(key) ?? null,
    key: (index) => Array.from(items.keys())[index] ?? null,
    removeItem: (key) => items.delete(key),
    setItem: (key, value) => items.set(key, value),
  };
}

function protocolPreparingTaskSnapshot(taskId: string, title: string) {
  return {
    ...protocolTaskSnapshot(taskId, title),
    revision: 2,
    preparation: { kind: "preparing" as const, steps: [] },
    agentConfig: { state: "loading" as const },
    agentCommands: { state: "loading" as const },
    sendCapability: {
      state: "loading" as const,
      blockers: [{ kind: "taskPreparing" as const, message: "Task Agent preparation is still running" }],
    },
  };
}

function protocolTaskSummary(taskId: string, title: string) {
  return {
    taskId: taskId as never,
    projectId: "project_1" as never,
    agentId: "codex" as never,
    title,
    status: "idle" as const,
    hasMessages: true,
    unread: false,
    updatedAt: "2026-05-22T00:00:00.000Z",
    lastActivity: "2026-05-22T00:00:00.000Z",
  };
}

function protocolAgents(ids: string[]) {
  return {
    agents: ids.map((agentId) => ({
      agentId: agentId as never,
      label: agentId === "codex" ? "Codex" : "Local Agent",
      status: "disconnected" as const,
      capabilities: { resumeTasks: false, deleteNativeSessions: false },
    })),
    defaultAgentId: ids[0] as never,
  };
}

function customSettingsAgent(id: string) {
  return {
    id,
    label: "Local Agent",
    enabled: true,
    scope: "global" as const,
    source_kind: "custom" as const,
    icon: "bot" as const,
    transport: "stdio" as const,
    status: "disconnected" as const,
    launch_label: "local-agent --stdio",
    command_line: "local-agent --stdio",
    env: [],
    description: "Custom ACP stdio Agent",
    capabilities: [],
    auth_methods: [],
  };
}
