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
  ATTACHMENT_RELEASE,
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
import {
  clearPendingTaskSendRecovery,
  readPendingTaskSendRecovery,
  savePendingTaskSendRecovery,
} from "../services/pendingTaskSendRecovery";
import { ComposerAttachmentResourceOwner } from "../services/attachmentResources";
import { projectIdForWorkspaceRoot } from "../state/projectIdentity";
import { appReducer } from "../state/appReducer";
import { createInitialState, toolDetailCacheKey, type AppState } from "../state/store";
import { newTaskPreparationKey } from "../state/newTaskPreparationContext";
import { PreparedTaskOwnership } from "./preparedTaskOwnership";

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
    for (const taskId of ["task_1", "task_2", "task_new"]) {
      clearPendingTaskSendRecovery("state_root_1", "test-client", taskId);
    }
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

    expect(dispatch).toHaveBeenNthCalledWith(1, expect.objectContaining({
      type: "submit:start",
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
    }));
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
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:submit",
      taskId: "task_1",
      input: { prompt: "Build the thing", context: [] },
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
    }));
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

  it("cleans the prepared Task and its attachments when cancellation wins preparation", async () => {
    const prepared = deferred<{ taskId: never; task: ReturnType<typeof protocolTaskSnapshot> }>();
    const dispatch = vi.fn();
    const release = vi.fn();
    const attachmentResources = new ComposerAttachmentResourceOwner({ release });
    const attachment = {
      kind: "file" as const,
      label: "notes.md",
      local_id: "attachment-1",
      app_server_handle_id: "handle-1" as AttachmentHandleId,
    };
    attachmentResources.reconcile({
      acceptedUserMessageIds: new Map(),
      acceptsAdoptions: true,
      retained: [{ taskId: "task_1", handleId: attachment.app_server_handle_id }],
      mountedTaskId: "task_1",
      protected: new Set(),
      taskSurfaceMounted: true,
    });
    const request = vi.fn(async (method: string) => {
      if (method === TASK_DISCARD) return { discarded: true };
      throw new Error(method);
    });
    const state = preparedNewTaskState();
    state.newTask.prompt = "Build";
    state.newTask.context = [attachment];
    const newTask = callbacks({
      attachmentResources,
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      pendingPreparedNewTask: () => prepared.promise,
      state,
    }).newTask;

    newTask.submit();
    newTask.cancel();
    prepared.resolve({ taskId: "task_1" as never, task: protocolTaskSnapshot("task_1", "New task") });
    await settlePromises();

    expect(request.mock.calls.filter(([method]) => method === TASK_DISCARD)).toEqual([
      [TASK_DISCARD, { taskId: "task_1" }],
    ]);
    expect(request).not.toHaveBeenCalledWith(TASK_SEND, expect.anything());
    expect(release).toHaveBeenCalledWith("task_1", ["handle-1"]);
    expect(dispatch).toHaveBeenCalledWith({ type: "taskInput:clear", taskId: "task_1" });
  });

  it("surfaces ambiguous prepared-Task cleanup without issuing a no-op cancel", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_DISCARD) throw new Error("connection closed");
      if (method === TASK_CANCEL) return { cancelled: true };
      throw new Error(method);
    });
    const state = createInitialState();
    state.newTask.selection.projectId = "project_1";
    state.newTask.submitting = true;
    const newTaskStartAttempt = {
      current: {
        cancelled: false,
        draft: { prompt: "Build", context: [] },
        taskId: "task_1" as never,
      },
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      newTaskStartAttempt,
      state,
    }).newTask.cancel();
    await settlePromises();

    expect(request).not.toHaveBeenCalledWith(TASK_CANCEL, { taskId: "task_1" });
    expect(dispatch).toHaveBeenCalledWith({
      type: "submit:error",
      message: "connection closed",
    });
    expect(dispatch).not.toHaveBeenCalledWith({ type: "taskInput:clear", taskId: "task_1" });
  });

  it("forgets resolver rows and recovery after a prepared Task is discarded", async () => {
    const dispatch = vi.fn();
    const release = vi.fn();
    const attachmentResources = new ComposerAttachmentResourceOwner({ release });
    const context = [{
      kind: "file" as const,
      label: "notes.md",
      local_id: "attachment-1",
      app_server_handle_id: "attachment-handle-1" as AttachmentHandleId,
    }];
    attachmentResources.reconcile({
      acceptedUserMessageIds: new Map(),
      acceptsAdoptions: false,
      retained: [{ taskId: "task_1", handleId: "attachment-handle-1" as AttachmentHandleId }],
      mountedTaskId: "task_1",
      protected: new Set(["task_1\u0000attachment-handle-1"]),
      taskSurfaceMounted: true,
    });
    savePendingTaskSendRecovery({
      clientInstanceId: "test-client" as never,
      idempotencyKey: "send-1" as never,
      message: { text: "Build", attachments: ["attachment-handle-1" as AttachmentHandleId] },
      renderState: { prompt: "Build", context },
      stateRootId: "state_root_1" as never,
      taskId: "task_1",
      taskRevision: 2,
    });
    const request = vi.fn(async (method: string) => {
      if (method === TASK_DISCARD) return { discardedTaskId: "task_1", tasks: { tasks: [], activeTaskId: null } };
      throw new Error(method);
    });
    const state = createInitialState();
    state.newTask.pending = { prompt: "Build", context, idempotencyKey: "send-1" as never };
    state.newTask.submitting = true;
    const attempt = {
      cancelled: false,
      draft: { prompt: "Build", context },
      taskId: "task_1" as never,
    };

    callbacks({
      attachmentResources,
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      newTaskStartAttempt: { current: attempt },
      state,
    }).newTask.cancel();
    await settlePromises();

    expect(readPendingTaskSendRecovery("state_root_1", "test-client", "task_1")).toBeUndefined();
    expect(release).toHaveBeenCalledWith("task_1", ["attachment-handle-1"]);
    expect(dispatch).toHaveBeenCalledWith({ type: "taskInput:clear", taskId: "task_1" });
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

  it("does not send through an empty prepared Task after its Agent and Project selection changed", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_DISCARD) return { discarded: true };
      if (method === TASK_CREATE) {
        return { task: protocolTaskSnapshotForContext("task_new", "project_2", "mock") };
      }
      if (method === TASK_SEND) {
        return {
          task: { ...protocolTaskSnapshotForContext("task_new", "project_2", "mock"), revision: 3 },
          turnId: "turn_1",
          userMessageId: "message_1",
        };
      }
      if (method === TASK_OPEN) return { task: protocolTaskSnapshot("task_old", "Old task") };
      throw new Error(method);
    });
    const state = preparedNewTaskState("task_old");
    state.newTask.prompt = "Use the new context";
    state.taskInputs.task_old = { prompt: "Old draft", context: [] };
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "mock",
      agentLabel: "Mock",
      projectId: "project_2",
      workspaceRoot: "",
      workspaceLabel: "Project 2",
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).newTask.submit();
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_DISCARD, { taskId: "task_old" });
    expect(dispatch).toHaveBeenCalledWith({ type: "taskInput:clear", taskId: "task_old" });
    expect(request).not.toHaveBeenCalledWith(TASK_OPEN, { taskId: "task_old" });
    expect(request).toHaveBeenCalledWith(TASK_CREATE, {
      projectId: "project_2",
      agentId: "mock",
    });
    expect(request).toHaveBeenCalledWith(TASK_SEND, expect.objectContaining({
      taskId: "task_new",
      message: { text: "Use the new context" },
    }));
  });

  it("attaches a pasted image from the new-task composer by preparing the Task first", async () => {
    const dispatch = vi.fn();
    const release = vi.fn();
    const attachmentResources = new ComposerAttachmentResourceOwner({ release });
    attachmentResources.reconcile({
      acceptedUserMessageIds: new Map(),
      acceptsAdoptions: true,
      retained: [],
      mountedTaskId: undefined,
      protected: new Set(),
      taskSurfaceMounted: true,
    });
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
      attachmentResources,
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
    expect(release).not.toHaveBeenCalled();
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("hands a direct paste from a superseded prepared Task to its replacement", async () => {
    const dispatch = vi.fn();
    const release = vi.fn();
    const attachmentResources = new ComposerAttachmentResourceOwner({ release });
    const staleAttachment = {
      kind: "file" as const,
      label: "old.md",
      local_id: "old-attachment",
      app_server_handle_id: "old-handle" as AttachmentHandleId,
    };
    attachmentResources.reconcile({
      acceptedUserMessageIds: new Map(),
      acceptsAdoptions: true,
      retained: [{ taskId: "task_old", handleId: staleAttachment.app_server_handle_id }],
      mountedTaskId: "task_old",
      protected: new Set(),
      taskSurfaceMounted: true,
    });
    const request = vi.fn(async (method: string) => {
      if (method === TASK_DISCARD) return { discarded: true };
      if (method === TASK_CREATE) {
        return { task: protocolTaskSnapshotForContext("task_new", "project_2", "mock") };
      }
      if (method === ATTACHMENT_CREATE_PASTED_IMAGE) {
        return { attachment: { handleId: "new-handle", label: "pasted.png" } };
      }
      throw new Error(method);
    });
    const state = preparedNewTaskState("task_old");
    state.taskInputs.task_old = { prompt: "Old draft", context: [staleAttachment] };
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "mock",
      agentLabel: "Mock",
      projectId: "project_2",
      workspaceRoot: "",
      workspaceLabel: "Project 2",
    };

    await expect(callbacks({
      attachmentResources,
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).newTask.fileBrowser?.attachPastedImage(
      new File([new Uint8Array([1, 2, 3])], "pasted.png", { type: "image/png" }),
      { prompt: "Explain the new image", context: [] },
    )).resolves.toBeUndefined();

    expect(request.mock.calls.filter(([method]) => method === TASK_DISCARD)).toEqual([
      [TASK_DISCARD, { taskId: "task_old" }],
    ]);
    expect(release).toHaveBeenCalledWith("task_old", ["old-handle"]);
    expect(request).toHaveBeenCalledWith(ATTACHMENT_CREATE_PASTED_IMAGE, expect.objectContaining({
      taskId: "task_new",
    }));
    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:attachment:addAppServer",
      taskId: "task_new",
      attachment: expect.objectContaining({ app_server_handle_id: "new-handle" }),
    });
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

  it("releases a file selection that returns after its prepared Task context is superseded", async () => {
    const selected = deferred<{ attachment: { handleId: string; label: string } }>();
    const dispatch = vi.fn();
    const release = vi.fn();
    const attachmentResources = new ComposerAttachmentResourceOwner({ release });
    attachmentResources.reconcile({
      acceptedUserMessageIds: new Map(),
      acceptsAdoptions: true,
      retained: [],
      mountedTaskId: "task_1",
      protected: new Set(),
      taskSurfaceMounted: true,
    });
    const request = vi.fn((method: string) => {
      if (method === ATTACHMENT_CREATE_FILE_REFERENCE) return selected.promise;
      return Promise.reject(new Error(method));
    });
    const state = preparedNewTaskState("task_1");
    let currentPreparationKey = newTaskPreparationKey(state);
    const fileBrowser = callbacks({
      attachmentResources,
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      currentNewTaskPreparationKey: () => currentPreparationKey,
      dispatch,
      state,
    }).newTask.fileBrowser;

    const attaching = fileBrowser?.attachFileReference("entry-1" as FileBrowserEntryId);
    currentPreparationKey = "project_2\u0000\u0000other-agent";
    selected.resolve({ attachment: { handleId: "late-handle", label: "late.md" } });

    await expect(attaching).rejects.toThrow("New Task context changed");
    expect(release).toHaveBeenCalledWith("task_1", ["late-handle"]);
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:attachment:addAppServer",
    }));
  });

  it("discards a Task prepared after its file picker context was superseded", async () => {
    const created = deferred<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const dispatch = vi.fn();
    const request = vi.fn((method: string) => {
      if (method === TASK_CREATE) return created.promise;
      if (method === TASK_DISCARD) return Promise.resolve({ discarded: true });
      if (method === ATTACHMENT_LIST_ROOTS) return Promise.resolve({ roots: [] });
      return Promise.reject(new Error(method));
    });
    const state = preparedNewTaskState();
    let currentPreparationKey = newTaskPreparationKey(state);
    const fileBrowser = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      currentNewTaskPreparationKey: () => currentPreparationKey,
      dispatch,
      state,
    }).newTask.fileBrowser;

    const listing = fileBrowser?.listRoots();
    await Promise.resolve();
    currentPreparationKey = "project_2\u0000\u0000other-agent";
    created.resolve({ task: protocolTaskSnapshot("task_late", "Late task") });

    await expect(listing).rejects.toThrow("New Task context changed");
    expect(request).toHaveBeenCalledWith(TASK_DISCARD, { taskId: "task_late" });
    expect(request).not.toHaveBeenCalledWith(ATTACHMENT_LIST_ROOTS, expect.anything());
    expect(dispatch).toHaveBeenCalledWith({ type: "taskInput:clear", taskId: "task_late" });
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({ type: "snapshot" }));
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:prompt",
      taskId: "task_late",
    }));
  });

  it("lets prepared-task ingestion transfer the latest prompt during file browsing", async () => {
    const created = deferred<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const dispatch = vi.fn();
    const request = vi.fn((method: string) => {
      if (method === TASK_CREATE) return created.promise;
      if (method === ATTACHMENT_LIST_ROOTS) return Promise.resolve({ roots: [] });
      return Promise.reject(new Error(method));
    });
    const state = preparedNewTaskState();
    state.newTask.prompt = "prompt captured before Task preparation";
    const fileBrowser = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).newTask.fileBrowser;

    const listing = fileBrowser?.listRoots();
    await Promise.resolve();
    const preparedTask = protocolTaskSnapshot("task_prepared", "Prepared task");
    created.resolve({
      task: {
        ...preparedTask,
        task: { ...preparedTask.task, hasMessages: false },
      },
    });
    await listing;

    expect(dispatch).toHaveBeenCalledWith({ type: "newTask:prepared", taskId: "task_prepared" });
    expect(dispatch).not.toHaveBeenCalledWith({
      type: "taskInput:prompt",
      taskId: "task_prepared",
      prompt: "prompt captured before Task preparation",
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
    state.snapshot.task.project_id = "project_1";
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
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:submit",
      taskId: "task_1",
      input: state.taskInputs.task_1,
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
    }));
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
      if (method === ATTACHMENT_RELEASE) {
        return { outcomes: [] };
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.snapshot.task.has_messages = false;
    state.snapshot.task.project_id = "project_1";
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
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
    expect(request).toHaveBeenCalledWith(ATTACHMENT_RELEASE, {
      taskId: "task_1",
      resources: [{ kind: "handle", id: "attachment-handle-image" }],
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
    state.snapshot.task.project_id = "project_1";
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

  it("adopts the prepared Task while keeping its draft pending until Send is accepted", async () => {
    const dispatch = vi.fn();
    const pendingSend = deferred<{
      task: ReturnType<typeof protocolTaskSnapshot>;
      turnId: string;
      userMessageId: string;
    }>();
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
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:submit",
      taskId: "task_1",
      input: { prompt: "Build the thing", context: [] },
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
    }));
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openTask",
      payload: { task_id: "task_1", title: "New task" },
    });

    pendingSend.resolve({
      turnId: "turn-1",
      userMessageId: "user-message",
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
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "taskSend:accepted",
      taskId: "task_1",
      userMessageId: "user-message",
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
    }));
    expect(postHostMessage).toHaveBeenCalledTimes(1);
  });

  it("settles an accepted first send before cancelling its active turn", async () => {
    const pendingSend = deferred<{
      task: ReturnType<typeof protocolTaskSnapshot>;
      turnId: string;
      userMessageId: string;
    }>();
    const dispatch = vi.fn();
    const request = vi.fn((method: string) => {
      if (method === TASK_CREATE) return Promise.resolve({ task: protocolTaskSnapshot("task_1", "New task") });
      if (method === TASK_SEND) return pendingSend.promise;
      if (method === TASK_DISCARD) return Promise.reject(new Error("Task already has an active turn."));
      if (method === TASK_CANCEL) return Promise.resolve({});
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
    const newTaskStartAttempt = { current: undefined };
    const newTask = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      newTaskStartAttempt,
      state,
    }).newTask;

    newTask.submit();
    await settlePromises();
    newTask.cancel();
    await settlePromises();

    expect(request).not.toHaveBeenCalledWith(TASK_CANCEL, expect.anything());

    pendingSend.resolve({
      task: { ...protocolTaskSnapshot("task_1", "Accepted task"), revision: 3 },
      turnId: "turn-1",
      userMessageId: "message-1",
    });
    await settlePromises();

    const submitted = dispatch.mock.calls
      .map(([action]) => action)
      .find((action) => action.type === "taskInput:submit");
    const acceptedIndex = dispatch.mock.calls.findIndex(([action]) => action.type === "taskSend:accepted");
    const cancelIndex = request.mock.calls.findIndex(([method]) => method === TASK_CANCEL);
    expect(dispatch.mock.calls[acceptedIndex]?.[0]).toEqual({
      type: "taskSend:accepted",
      taskId: "task_1",
      idempotencyKey: submitted.idempotencyKey,
      userMessageId: "message-1",
    });
    expect(dispatch.mock.invocationCallOrder[acceptedIndex])
      .toBeLessThan(request.mock.invocationCallOrder[cancelIndex]);
  });

  it("does not let an older first-send rejection reclaim a newer prepared lease", async () => {
    const pendingSend = deferred<never>();
    const request = vi.fn((method: string) => {
      if (method === TASK_OPEN) {
        return Promise.resolve({ task: protocolTaskSnapshot("task_a", "First draft") });
      }
      if (method === TASK_SEND) return pendingSend.promise;
      return Promise.reject(new Error(method));
    });
    const state = preparedNewTaskState("task_a");
    state.taskInputs.task_a = { prompt: "Send A", context: [] };
    const preparedTaskOwnership = new PreparedTaskOwnership();
    const newTask = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      preparedTaskOwnership,
      state,
    }).newTask;

    newTask.submit();
    await settlePromises();
    const leaseB = preparedTaskOwnership.claim({
      preparationKey: "context-b",
      taskId: "task_b" as never,
    });
    pendingSend.reject(new AppServerProtocolError({
      error: { code: "conflict", message: "Send A rejected", recoverable: true },
    }));
    await settlePromises();

    expect(preparedTaskOwnership.currentLease()).toBe(leaseB);
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
      const request = vi.fn(async (method: string, _params?: unknown) => {
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

  it("retries the first new-task send from authoritative revision-conflict state", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string, _params?: unknown) => {
      if (method === TASK_CREATE) return { task: { ...protocolTaskSnapshot("task_1", "New task"), revision: 1 } };
      if (method === TASK_SEND && request.mock.calls.filter(([called]) => called === TASK_SEND).length === 1) {
        throw new AppServerProtocolError({
          error: {
            code: "conflict",
            message: "Task changed before the message was sent",
            recoverable: true,
            target: {
              field: "taskRevision",
              currentTask: { ...protocolTaskSnapshot("task_1", "New task"), revision: 2 },
            },
          },
        });
      }
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
    expect(request).toHaveBeenNthCalledWith(3, TASK_SEND, {
      taskId: "task_1",
      taskRevision: 2,
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
      message: { text: "Build the thing" },
    });
    const firstSend = request.mock.calls[1][1] as { idempotencyKey: string };
    const retrySend = request.mock.calls[2][1] as { idempotencyKey: string };
    expect(retrySend.idempotencyKey).toBe(firstSend.idempotencyKey);
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({ type: "taskInput:error" }));
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openTask",
      payload: {
        task_id: "task_1",
        title: "New task",
      },
    });
  });

  it("adopts the prepared Task route before the first send outcome is known", async () => {
    const pendingSend = deferred<{
      task: ReturnType<typeof protocolTaskSnapshot>;
      turnId: string;
      userMessageId: string;
    }>();
    const dispatch = vi.fn();
    const request = vi.fn((method: string) => {
      if (method === TASK_CREATE) {
        return Promise.resolve({ task: protocolTaskSnapshot("task_1", "New task") });
      }
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
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openTask",
      payload: {
        task_id: "task_1",
        title: "New task",
      },
    });

    pendingSend.resolve({
      task: { ...protocolTaskSnapshot("task_1", "Accepted task"), revision: 3 },
      turnId: "turn_1",
      userMessageId: "message_1",
    });
    await settlePromises();
  });

  it("keeps the prepared Task route and restores its draft when first send is ambiguous", async () => {
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
    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:sendUncertain",
      taskId: "task_1",
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
      message: "Send status is unknown. Retry sends this exact message.",
    });
    expect(dispatch).toHaveBeenCalledWith({ type: "submit:error", message: "send failed" });
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openTask",
      payload: { task_id: "task_1", title: "New task" },
    });
  });

  it("keeps pending new-task send recovery after an ambiguous transport failure", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
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

      expect(readPendingTaskSendRecovery("state_root_1", "test-client", "task_1")).toMatchObject({
        idempotencyKey: expect.stringMatching(/^frontend-send-/),
        renderState: { prompt: "Build the thing", context: [] },
      });
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("retries a new-task send with the persisted idempotency key", async () => {
    vi.stubGlobal("sessionStorage", memoryStorage());
    try {
      const pendingRetry = deferred<never>();
      const request = vi.fn(async (method: string, _params?: unknown) => {
        if (method === TASK_OPEN) return { task: protocolTaskSnapshot("task_1", "New task") };
        if (method === TASK_SEND && request.mock.calls.filter(([called]) => called === TASK_SEND).length === 1) {
          throw new Error("connection closed");
        }
        if (method === TASK_SEND) return pendingRetry.promise;
        throw new Error(method);
      });
      const state = createInitialState();
      state.snapshot = snapshot("task_1");
      state.snapshot.task.has_messages = false;
      state.snapshot.task.project_id = "project_1";
      state.newTask.selection = {
        ...state.newTask.selection,
        agentId: "codex",
        projectId: "project_1",
        workspaceRoot: "/workspace",
        workspaceLabel: "workspace",
      };
      state.taskInputs.task_1 = { prompt: "Build the thing", context: [] };
      const controllerCallbacks = callbacks({
        backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
        state,
      });

      controllerCallbacks.newTask.submit();
      await settlePromises();
      controllerCallbacks.newTask.submit();
      await settlePromises();

      const sends = request.mock.calls.filter(([method]) => method === TASK_SEND);
      expect(sends).toHaveLength(2);
      const firstSend = sends[0]?.[1] as { idempotencyKey: string };
      const secondSend = sends[1]?.[1] as { idempotencyKey: string };
      expect(secondSend).toMatchObject({
        idempotencyKey: firstSend.idempotencyKey,
        message: { text: "Build the thing" },
        taskId: "task_1",
      });
      expect(readPendingTaskSendRecovery("state_root_1", "test-client", "task_1")?.idempotencyKey)
        .toBe(secondSend.idempotencyKey);
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
    state.snapshot.agent_config = editableConfigOptions();

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

  it("ingests config option results even when the event stream is subscribed", async () => {
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
    state.snapshot.agent_config = editableConfigOptions();

    callbacks({ backendConnection, dispatch, state }).task.selectConfigOption("model", "gpt-5");
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SET_CONFIG_OPTION, expect.objectContaining({
      taskId: "task_1",
      configId: "model",
      value: "gpt-5",
    }));
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: "snapshot", intent: "refresh" }));
  });

  it("does not mutate config options while the authoritative catalog is changing", async () => {
    const dispatch = vi.fn();
    const request = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.snapshot.agent_config = {
      ...editableConfigOptions(),
      pending_change: {
        mutation_id: "mutation_1",
        option_id: "model",
        requested_value: "gpt-5",
      },
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task.selectConfigOption("model", "gpt-5");
    await settlePromises();

    expect(request).not.toHaveBeenCalled();
    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:error",
      taskId: "task_1",
      message: "Configuration options are not currently editable.",
    });
  });

  it("refreshes an existing Task after a config mutation error clears backend pending state", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_SET_CONFIG_OPTION) {
        throw new AppServerProtocolError({
          error: { code: "internal", message: "Agent rejected the option", recoverable: true },
        });
      }
      if (method === TASK_OPEN) {
        return { task: { ...protocolTaskSnapshot("task_1", "Task"), revision: 4 } };
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.activeTaskId = "task_1";
    state.snapshot = snapshot("task_1");
    state.snapshot.agent_config = editableConfigOptions();

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task.selectConfigOption("model", "gpt-5");
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_OPEN, { taskId: "task_1" });
    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:error",
      taskId: "task_1",
      message: "Agent rejected the option",
    });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: "snapshot", intent: "refresh" }));
  });

  it("keeps an exact send pending when an older config mutation fails", async () => {
    const configMutation = deferred<never>();
    const request = vi.fn((method: string) => {
      if (method === TASK_SET_CONFIG_OPTION) return configMutation.promise;
      if (method === TASK_OPEN) {
        return Promise.resolve({ task: { ...protocolTaskSnapshot("task_1", "Task"), revision: 4 } });
      }
      throw new Error(method);
    });
    let renderedState = createInitialState();
    renderedState.activeTaskId = "task_1";
    renderedState.snapshot = snapshot("task_1");
    renderedState.snapshot.agent_config = editableConfigOptions();
    const callbackState = renderedState;
    const dispatch = vi.fn((action) => {
      renderedState = appReducer(renderedState, action);
    });
    const task = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state: callbackState,
    }).task;

    task.selectConfigOption("model", "gpt-5");
    dispatch({
      type: "taskInput:submit",
      taskId: "task_1",
      input: { prompt: "Send exactly once", context: [] },
      idempotencyKey: "send-attempt-1" as never,
    });
    configMutation.reject(new Error("Agent rejected the option"));
    await settlePromises();

    expect(renderedState.taskInputs.task_1.pending).toMatchObject({
      idempotencyKey: "send-attempt-1",
      prompt: "Send exactly once",
      state: "sending",
    });
    expect(renderedState.taskInputs.task_1.error).toBe("Agent rejected the option");
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
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
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
    const request = vi.fn(async () => ({ outcomes: [] }));
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
    expect(request).toHaveBeenCalledWith(ATTACHMENT_RELEASE, {
      taskId: "task_1",
      resources: [{ kind: "handle", id: "attachment-handle-1" }],
    });
  });

  it("releases prepared new-task attachment handles when their composer rows are removed", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({ outcomes: [] }));
    const state = createInitialState();
    state.snapshot = {
      ...snapshot("task_1"),
      task: { ...snapshot("task_1").task, has_messages: false },
    };
    state.taskInputs.task_1 = {
      prompt: "Draft",
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
    }).newTask.removeAttachment("ctx_1");
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:attachment:remove",
      taskId: "task_1",
      attachmentId: "ctx_1",
    });
    expect(request).toHaveBeenCalledWith(ATTACHMENT_RELEASE, {
      taskId: "task_1",
      resources: [{ kind: "handle", id: "attachment-handle-1" }],
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

  it("releases a selected handle returned after the Task composer was abandoned", async () => {
    const dispatch = vi.fn();
    const release = vi.fn();
    const attachmentResources = new ComposerAttachmentResourceOwner({ release });
    attachmentResources.reconcile({
      acceptedUserMessageIds: new Map(),
      acceptsAdoptions: true,
      retained: [],
      mountedTaskId: "task_1",
      protected: new Set(),
      taskSurfaceMounted: true,
    });
    const selected = deferred<{ attachment: { handleId: string; label: string } }>();
    const request = vi.fn(() => selected.promise);
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    const attaching = callbacks({
      attachmentResources,
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task.fileBrowser?.attachFileReference("entry-1" as FileBrowserEntryId);
    attachmentResources.reconcile({
      acceptedUserMessageIds: new Map(),
      acceptsAdoptions: true,
      retained: [],
      mountedTaskId: "task_2",
      protected: new Set(),
      taskSurfaceMounted: true,
    });
    selected.resolve({ attachment: { handleId: "attachment-handle-1", label: "notes.md" } });
    await attaching;

    expect(release).toHaveBeenCalledWith("task_1", ["attachment-handle-1"]);
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:attachment:addAppServer",
    }));
  });

  it("rejects a selection response that arrives after Send starts", async () => {
    const selected = deferred<{ attachment: { handleId: string; label: string } }>();
    const sent = deferred<{
      task: ReturnType<typeof protocolTaskSnapshot>;
      turnId: string;
      userMessageId: string;
    }>();
    const dispatch = vi.fn();
    const release = vi.fn();
    const attachmentResources = new ComposerAttachmentResourceOwner({ release });
    attachmentResources.reconcile({
      acceptedUserMessageIds: new Map(),
      acceptsAdoptions: true,
      retained: [],
      mountedTaskId: "task_1",
      protected: new Set(),
      taskSurfaceMounted: true,
    });
    const request = vi.fn((method: string) => {
      if (method === ATTACHMENT_CREATE_FILE_REFERENCE) return selected.promise;
      if (method === TASK_SEND) return sent.promise;
      return Promise.reject(new Error(method));
    });
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.taskInputs.task_1 = { prompt: "Start now", context: [] };
    const task = callbacks({
      attachmentResources,
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).task;
    const attaching = task.fileBrowser?.attachFileReference("entry-1" as FileBrowserEntryId);

    task.sendPrompt();
    selected.resolve({ attachment: { handleId: "late-handle", label: "late.md" } });
    await attaching;

    expect(release).toHaveBeenCalledWith("task_1", ["late-handle"]);
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:attachment:addAppServer",
    }));

    sent.resolve({
      task: protocolTaskSnapshot("task_1", "Task"),
      turnId: "turn-1",
      userMessageId: "message-1",
    });
    await settlePromises();
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

  it("releases an embedded candidate when confirmation leaves it non-sendable", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === ATTACHMENT_CREATE_EMBEDDED_CANDIDATE) {
        return { candidate: { candidateId: "candidate-1", label: "notes.md" } };
      }
      if (method === ATTACHMENT_CONFIRM_EMBEDDED) {
        return {
          attachments: [],
          errors: [{
            candidateId: "candidate-1",
            code: "tooLarge",
            message: "Attachment is too large.",
          }],
        };
      }
      if (method === ATTACHMENT_RELEASE) {
        return { outcomes: [] };
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    const task = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      state,
    }).task;

    await expect(task.fileBrowser?.attachEmbedded("entry-1" as FileBrowserEntryId))
      .rejects.toThrow("Attachment is too large.");
    await settlePromises();

    expect(request).toHaveBeenCalledWith(ATTACHMENT_RELEASE, {
      taskId: "task_1",
      resources: [{ kind: "candidate", id: "candidate-1" }],
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

  it("waits for an in-flight send to settle before cancelling that Task", async () => {
    const dispatch = vi.fn();
    const pendingSend = deferred<{
      task: ProtocolTaskSnapshot;
      userMessageId: string;
    }>();
    const request = vi.fn((method: string) => {
      if (method === TASK_SEND) return pendingSend.promise;
      if (method === TASK_CANCEL) {
        return Promise.resolve({ task: protocolTaskSnapshot("task_1", "Cancelled") });
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.taskInputs.task_1 = { prompt: "Start then stop", context: [] };
    const dependencies = {
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    };

    callbacks(dependencies).task.sendPrompt();
    const submit = dispatch.mock.calls
      .map(([action]) => action)
      .find((action) => action.type === "taskInput:submit");
    state.taskInputs.task_1 = {
      prompt: "Start then stop",
      context: [],
      pending: {
        prompt: "Start then stop",
        context: [],
        idempotencyKey: submit.idempotencyKey,
        state: "sending",
      },
    };

    callbacks(dependencies).task.cancel();

    expect(request).toHaveBeenCalledTimes(1);
    expect(request).not.toHaveBeenCalledWith(TASK_CANCEL, { taskId: "task_1" });

    pendingSend.resolve({
      task: protocolTaskSnapshot("task_1", "Started"),
      userMessageId: "accepted-user-message",
    });
    await settlePromises();

    const acceptedIndex = dispatch.mock.calls
      .findIndex(([action]) => action.type === "taskSend:accepted");
    const cancelIndex = request.mock.calls.findIndex(([method]) => method === TASK_CANCEL);
    expect(acceptedIndex).toBeGreaterThanOrEqual(0);
    expect(cancelIndex).toBeGreaterThanOrEqual(0);
    expect(dispatch.mock.invocationCallOrder[acceptedIndex])
      .toBeLessThan(request.mock.invocationCallOrder[cancelIndex]);
  });

  it("surfaces an error when cancel has no BackendConnection", () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({ dispatch, state }).task.cancel();

    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:cancelError",
      taskId: "task_1",
      message: "App Server connection unavailable.",
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("locks the exact typed prompt after a send outcome becomes unknown", async () => {
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
      type: "taskInput:sendUncertain",
      taskId: "task_1",
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
      message: "Send status is unknown. Retry sends this exact message.",
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
      type: "taskInput:sendError",
      taskId: "task_1",
      idempotencyKey: expect.stringMatching(/^frontend-send-/),
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
      type: "taskInput:cancelError",
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
      sessionId: "native_1",
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

  it("discards a prepared Task exactly once when opening an existing Task", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_DISCARD) return { discarded: true };
      throw new Error(method);
    });
    const state = preparedNewTaskState("task_prepared");
    const navigation = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).navigation;

    navigation.openTask("task_existing");
    navigation.openTask("task_existing");
    await settlePromises();

    expect(request.mock.calls.filter(([method]) => method === TASK_DISCARD)).toEqual([
      [TASK_DISCARD, { taskId: "task_prepared" }],
    ]);
  });

  it("discards a prepared Task exactly once when opening Settings", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_DISCARD) return { discarded: true };
      throw new Error(method);
    });
    const state = preparedNewTaskState("task_prepared");
    const navigation = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).navigation;

    navigation.openSettings();
    navigation.openSettings();
    await settlePromises();

    expect(request.mock.calls.filter(([method]) => method === TASK_DISCARD)).toEqual([
      [TASK_DISCARD, { taskId: "task_prepared" }],
    ]);
  });

  it("discards the prepared Task before adopting a Native Session", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_DISCARD) return { discarded: true };
      if (method === TASK_ADOPT_NATIVE_SESSION) {
        return { task: protocolTaskSnapshot("task_adopted", "Native Session") };
      }
      throw new Error(method);
    });
    const state = preparedNewTaskState("task_prepared");

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

    expect(request.mock.calls.filter(([method]) => method === TASK_DISCARD)).toEqual([
      [TASK_DISCARD, { taskId: "task_prepared" }],
    ]);
    expect(request.mock.calls.findIndex(([method]) => method === TASK_DISCARD))
      .toBeLessThan(request.mock.calls.findIndex(([method]) => method === TASK_ADOPT_NATIVE_SESSION));
    expect(request).toHaveBeenCalledWith(TASK_ADOPT_NATIVE_SESSION, expect.objectContaining({
      nativeSessionId: "native_1",
    }));
  });

  it("does not discard a prepared Task after its first send has started", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_DISCARD) return { discarded: true };
      throw new Error(method);
    });
    const state = preparedNewTaskState("task_prepared");
    const preparedTaskOwnership = new PreparedTaskOwnership();
    const lease = preparedTaskOwnership.claim({
      preparationKey: newTaskPreparationKey(state) as string,
      taskId: "task_prepared" as never,
    });
    preparedTaskOwnership.protectSend(lease, "send-prepared");

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      preparedTaskOwnership,
      newTaskStartAttempt: {
        current: {
          cancelled: false,
          draft: { prompt: "Sending", context: [] },
          sendInFlight: true,
          taskId: "task_prepared" as never,
        },
      },
      state,
    }).navigation.openSettings();
    await settlePromises();

    expect(request).not.toHaveBeenCalledWith(TASK_DISCARD, expect.anything());
  });

  it("does not let a stale prepared snapshot replace a newer ownership lease", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === TASK_DISCARD) return { discarded: true };
      throw new Error(method);
    });
    const state = preparedNewTaskState("task_stale");
    const preparedTaskOwnership = new PreparedTaskOwnership();
    const currentLease = preparedTaskOwnership.claim({
      preparationKey: "newer-context",
      taskId: "task_newer" as never,
    });

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      preparedTaskOwnership,
      state,
    }).navigation.openSettings();
    await settlePromises();

    expect(request).not.toHaveBeenCalledWith(TASK_DISCARD, expect.anything());
    expect(preparedTaskOwnership.currentLease()).toBe(currentLease);
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

  it("cancels native-session opening without letting its late result hijack the New Task surface", async () => {
    const adopted = deferred<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const dispatch = vi.fn();
    let generation = 0;
    const beginNavigationChange = vi.fn(() => ++generation);
    const state = preparedNewTaskState();
    const controllerCallbacks = callbacks({
      backendConnection: {
        request: vi.fn(() => adopted.promise) as unknown as BackendConnection["request"],
        respond: vi.fn(),
      },
      beginNavigationChange,
      currentNavigationGeneration: () => generation,
      dispatch,
      state,
    });

    controllerCallbacks.navigation.openNativeSession({
      cwd: "/workspace",
      session_id: "native_1",
      title: "Native Session",
      updated_at: "2026-06-27T00:00:00Z",
    });
    state.newTask.submitting = true;
    state.newTask.nativeSessions.adoptingSessionId = "native_1";
    controllerCallbacks.newTask.cancel();
    adopted.resolve({ task: protocolTaskSnapshot("task_adopted", "Native Session") });
    await settlePromises();

    expect(beginNavigationChange).toHaveBeenCalledTimes(2);
    expect(dispatch).toHaveBeenCalledWith({ type: "submit:cancel" });
    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:nativeSessions:remove",
      sessionId: "native_1",
    });
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({ type: "snapshot", intent: "open" }));
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "surface.openTask" }));
  });

  it("does not let a stale native-session result unlock a newer first send", async () => {
    const adopted = deferred<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    let generation = 0;
    let renderedState = preparedNewTaskState();
    const callbackState = renderedState;
    const dispatch = vi.fn((action) => {
      renderedState = appReducer(renderedState, action);
    });
    const controllerCallbacks = callbacks({
      backendConnection: {
        request: vi.fn(() => adopted.promise) as unknown as BackendConnection["request"],
        respond: vi.fn(),
      },
      beginNavigationChange: () => ++generation,
      currentNavigationGeneration: () => generation,
      dispatch,
      state: callbackState,
    });

    controllerCallbacks.navigation.openNativeSession({
      cwd: "/workspace",
      session_id: "native_1",
      title: "Native Session",
      updated_at: "2026-06-27T00:00:00Z",
    });
    callbackState.newTask.submitting = true;
    callbackState.newTask.nativeSessions.adoptingSessionId = "native_1";
    controllerCallbacks.newTask.cancel();
    dispatch({
      type: "submit:start",
      prompt: "Start this newer Task",
      context: [],
      idempotencyKey: "send-attempt-new" as never,
    });

    adopted.resolve({ task: protocolTaskSnapshot("task_adopted", "Native Session") });
    await settlePromises();

    expect(renderedState.newTask.submitting).toBe(true);
    expect(renderedState.newTask.pending?.idempotencyKey).toBe("send-attempt-new");
    expect(renderedState.newTask.nativeSessions.adoptingSessionId).toBeUndefined();
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
      archived: false,
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
      archived: false,
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
      archived: true,
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

  it("refreshes a prepared Task after a config mutation error clears backend pending state", async () => {
    let renderedState = preparedNewTaskState("task-prepared");
    renderedState.activeTaskId = "task-prepared";
    const dispatch = vi.fn((action) => {
      renderedState = appReducer(renderedState, action);
    });
    const request = vi.fn(async (method: string) => {
      if (method === TASK_SET_CONFIG_OPTION) {
        throw new AppServerProtocolError({
          error: { code: "internal", message: "Agent rejected the option", recoverable: true },
        });
      }
      if (method === TASK_OPEN) {
        return { task: { ...protocolTaskSnapshot("task-prepared", "New task"), revision: 4 } };
      }
      throw new Error(method);
    });
    const state = renderedState;

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      dispatch,
      state,
    }).newTask.selectConfigOption("model", "gpt-5");
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_OPEN, { taskId: "task-prepared" });
    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:configOptions:error",
      message: "Unable to update Agent option.",
    });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: "snapshot", intent: "refresh" }));
    expect(renderedState.newTask.configOptionsError).toBe("Unable to update Agent option.");
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
    const createChatPageRequestGeneration = vi.fn(() => 37);
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

    const requestGeneration = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"], respond: vi.fn() },
      createChatPageRequestGeneration,
      dispatch,
      state,
    }).task.loadChatPage("cursor_1");
    await settlePromises();

    expect(requestGeneration).toBe(37);
    expect(dispatch).toHaveBeenNthCalledWith(1, {
      type: "chatPage:start",
      taskId: "task_1",
      requestGeneration: 37,
    });
    expect(request).toHaveBeenCalledWith(TASK_CHAT_PAGE, {
      taskId: "task_1",
      beforeCursor: "cursor_1",
      limit: 50,
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "chatPage:result",
      taskId: "task_1",
      requestGeneration: 37,
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
  attachmentResources,
  backendConnection,
  beginNavigationChange = vi.fn(() => 1),
  createChatPageRequestGeneration = vi.fn(() => 1),
  currentNavigationGeneration = vi.fn(() => 1),
  state = createInitialState(),
  currentNewTaskPreparationKey = () => newTaskPreparationKey(state),
  dispatch = vi.fn(),
  latestOptionsRequestKey = { current: undefined as string | undefined },
  newTaskStartAttempt = { current: undefined },
  pendingPreparedNewTask = vi.fn(() => undefined),
  preparedTaskOwnership,
  requestNativeSessions = vi.fn(),
  setAgents = vi.fn(),
  setPreferences = vi.fn(),
}: Partial<Parameters<typeof createAppCallbacks>[0]> = {}) {
  state.appServerStateRootId ??= "state_root_1";
  return createAppCallbacks({
    attachmentResources,
    backendConnection,
    beginNavigationChange,
    clientInstanceId: "test-client",
    createChatPageRequestGeneration,
    createSnapshotRequestId: vi.fn(() => 91),
    currentNavigationGeneration,
    currentNewTaskPreparationKey,
    dispatch,
    latestOptionsRequestKey,
    newTaskStartAttempt,
    pendingPreparedNewTask,
    preparedTaskOwnership,
    requestNativeSessions,
    setAgents,
    setPreferences,
    state,
  });
}

function preparedNewTaskState(taskId?: string) {
  const state = createInitialState();
  state.newTask.selection = {
    ...state.newTask.selection,
    agentId: "codex",
    agentLabel: "Codex",
    projectId: "project_1",
    workspaceRoot: "/workspace",
    workspaceLabel: "workspace",
  };
  if (taskId) {
    state.snapshot = {
      ...snapshot(taskId),
      task: {
        ...snapshot(taskId).task,
        project_id: "project_1",
        has_messages: false,
      },
    };
  }
  return state;
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

function editableConfigOptions(): NonNullable<TaskSnapshot["agent_config"]> {
  return {
    agent_id: "codex",
    status: "ready",
    options: [{
      id: "model",
      label: "Model",
      current_value: "gpt",
      values: [
        { id: "gpt", label: "GPT" },
        { id: "gpt-5", label: "GPT-5" },
      ],
    }],
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

function protocolTaskSnapshotForContext(taskId: string, projectId: string, agentId: string) {
  const task = protocolTaskSnapshot(taskId, "New task");
  return {
    ...task,
    task: {
      ...task.task,
      projectId: projectId as never,
      agentId: agentId as never,
    },
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
