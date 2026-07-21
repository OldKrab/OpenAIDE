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
  CLIENT_HEARTBEAT,
  PENDING_REQUEST_RESOLVE,
  SETTINGS_GET_AGENT_DETAILS,
  SETTINGS_GET_MCP_SERVERS,
  SETTINGS_GET_SKILLS,
  SETTINGS_UPDATE_PREFERENCES,
  SETTINGS_UPDATE_RUNTIME,
  STATE_SUBSCRIBE,
  STATE_UNSUBSCRIBE,
  TASK_ADOPT_NATIVE_SESSION,
  TASK_CANCEL,
  TASK_CHAT_PAGE,
  TASK_ACQUIRE,
  TASK_RELEASE,
  TASK_LIST,
  TASK_OPEN,
  TASK_SEND,
  TASK_SET_ARCHIVED,
  TASK_SET_CONFIG_OPTION,
  type AttachmentHandleId,
  type BackendConnection,
  type FileBrowserEntryId,
  type FileBrowserRootId,
  type TaskSnapshot as ProtocolTaskSnapshot,
} from "@openaide/app-server-client";
import { createAppCallbacks } from "./appControllerCallbacks";
import { ComposerAttachmentResourceOwner } from "../services/attachmentResources";
import { projectIdForWorkspaceRoot } from "../state/projectIdentity";
import { appReducer } from "../state/appReducer";
import { createInitialState, type AppState } from "../state/store";
import { newTaskPreparationKey } from "../state/newTaskPreparationContext";
import { AsyncOperationOwner } from "../state/asyncOperationOwner";
import { NewTaskController } from "./newTaskController";

const postHostMessage = vi.fn();
const beginAgentSecretTransaction = vi.fn();
const frontendShellState = vi.hoisted(() => ({ files: undefined as undefined | {
  kind: "webUpload";
  upload: ReturnType<typeof vi.fn>;
} }));

vi.mock("../services/hostBridge", () => ({
  openNewTaskSurface: (projectId?: string) => postHostMessage(projectId
    ? { type: "surface.openNewTask", payload: { project_id: projectId } }
    : { type: "surface.openNewTask" }),
  openSettingsSurface: () => postHostMessage({ type: "surface.openSettings" }),
  openTaskSurface: (taskId: string, title?: string) => postHostMessage({
    type: "surface.openTask",
    payload: { task_id: taskId, ...(title ? { title } : {}) },
  }),
  postHostMessage: (message: unknown) => postHostMessage(message),
  replaceSettingsTabRoute: vi.fn(),
}));

vi.mock("../services/agentSecretTransaction", () => ({
  beginAgentSecretTransaction: (changes: unknown) => beginAgentSecretTransaction(changes),
}));

vi.mock("../services/frontendShell", () => ({
  currentFrontendShell: () => frontendShellState.files ? { files: frontendShellState.files } : undefined,
}));

describe("app controller callbacks", () => {
  beforeEach(() => {
    postHostMessage.mockClear();
    beginAgentSecretTransaction.mockReset();
    beginAgentSecretTransaction.mockResolvedValue({
      commit: vi.fn(async () => undefined),
      rollback: vi.fn(async () => undefined),
    });
    frontendShellState.files = undefined;
  });

  it("starts a New Task with Agent defaults before Send", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_ACQUIRE) return { task: protocolTaskSnapshot("task_1", "New task") };
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
      workspaceRoot: "/workspace",
      workspaceLabel: "workspace",
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).newTask.submit();
    await settlePromises();

    expect(dispatch).toHaveBeenNthCalledWith(1, expect.objectContaining({
      type: "submit:start",
    }));
    expect(request).toHaveBeenNthCalledWith(1, TASK_ACQUIRE, {
      projectId: "project_1",
      agentId: "codex",
    });
    expect(request).toHaveBeenNthCalledWith(2, TASK_SEND, {
      taskId: "task_1",
      message: { text: "Build the thing" },
    });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: "snapshot", intent: "open" }));
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:submit",
      taskId: "task_1",
      input: { prompt: "Build the thing", context: [] },
    }));
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openTask",
      payload: {
        task_id: "task_1",
        title: "New task",
      },
    });
  });

  it("sends uploaded file handles with the first New Task message", async () => {
    const state = preparedNewTaskState("task_1");
    state.taskInputs.task_1 = {
      prompt: "Inspect this file",
      context: [{
        local_id: "file-1",
        kind: "file",
        label: "notes.md",
        app_server_handle_id: "attachment-handle-1" as AttachmentHandleId,
      }],
    };
    const newTaskController = new NewTaskController();
    newTaskController.retain({
      preparationKey: newTaskPreparationKey(state) as string,
      snapshot: state.snapshot as TaskSnapshot,
    });
    const request = vi.fn(async (method: string) => {
      if (method === TASK_SEND) {
        return { task: { ...protocolTaskSnapshot("task_1", "New task"), revision: 4 } };
      }
      throw new Error(method);
    });

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      newTaskController,
      state,
    }).newTask.submit();
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SEND, {
      taskId: "task_1",
      message: {
        text: "Inspect this file",
        attachments: ["attachment-handle-1"],
      },
    });
  });

  it("stops a pre-send startup by discarding its prepared Task", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_RELEASE) return { discardedTaskId: "task_1" };
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      newTaskStartAttempt,
      state,
    }).newTask.cancel();
    await settlePromises();

    expect(attempt.cancelled).toBe(true);
    expect(newTaskStartAttempt.current).toBeUndefined();
    expect(dispatch).toHaveBeenCalledWith({ type: "submit:cancel" });
    expect(request).toHaveBeenCalledWith(TASK_RELEASE, { taskId: "task_1" });
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openNewTask",
      payload: { project_id: "project_1" },
    });
  });


  it("surfaces ambiguous prepared-Task cleanup without issuing a no-op cancel", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_RELEASE) throw new Error("connection closed");
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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

  it("includes a new workspace root when creating a task for an unseen project", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_ACQUIRE) return { task: protocolTaskSnapshot("task_1", "New task") };
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).newTask.submit();
    await settlePromises();

    expect(request).toHaveBeenNthCalledWith(1, TASK_ACQUIRE, {
      projectId: "project-fe42cc83da346a18",
      agentId: "codex",
      workspaceRoot: "/workspace/new-app",
    });
  });

  it("does not send through an empty prepared Task after its Agent and Project selection changed", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_RELEASE) return { discarded: true };
      if (method === TASK_ACQUIRE) {
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).newTask.submit();
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_RELEASE, { taskId: "task_old" });
    expect(dispatch).toHaveBeenCalledWith({ type: "taskInput:clear", taskId: "task_old" });
    expect(request).not.toHaveBeenCalledWith(TASK_OPEN, { taskId: "task_old" });
    expect(request).toHaveBeenCalledWith(TASK_ACQUIRE, {
      projectId: "project_2",
      agentId: "mock",
    });
    expect(request).toHaveBeenCalledWith(TASK_SEND, expect.objectContaining({
      taskId: "task_new",
      message: { text: "Use the new context" },
    }));
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
    const asyncOperations = new AsyncOperationOwner();
    const backendConnection = {
      request: request as unknown as BackendConnection["request"],
    };
    const fileBrowser = callbacks({
      attachmentResources,
      asyncOperations,
      backendConnection,
      dispatch,
      state,
    }).newTask.fileBrowser;

    const attaching = fileBrowser?.attachFileReference("entry-1" as FileBrowserEntryId);
    await vi.waitFor(() => expect(request).toHaveBeenCalledWith(ATTACHMENT_CREATE_FILE_REFERENCE, {
      taskId: "task_1",
      entryId: "entry-1",
    }));
    const nextState = {
      ...state,
      newTask: {
        ...state.newTask,
        selection: { ...state.newTask.selection, projectId: "project_2", agentId: "other-agent" },
      },
    };
    const newerFileBrowser = callbacks({ asyncOperations, backendConnection, dispatch, state: nextState }).newTask.fileBrowser;
    expect(newerFileBrowser?.ownerKey).not.toBe(fileBrowser?.ownerKey);
    selected.resolve({ attachment: { handleId: "late-handle", label: "late.md" } });

    await expect(attaching).rejects.toThrow("New Task context changed");
    expect(release).toHaveBeenCalledWith("task_1", ["late-handle"]);
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:attachment:addAppServer",
    }));
  });

  it("keeps ready agent options when file selection began before preparation completed", async () => {
    const request = vi.fn((method: string) => {
      if (method === ATTACHMENT_LIST_ROOTS) return Promise.resolve({ roots: [] });
      return Promise.reject(new Error(method));
    });
    const state = preparedNewTaskState();
    const newTaskController = new NewTaskController();
    const fileBrowser = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      newTaskController,
      state,
    }).newTask.fileBrowser;
    const readyState = preparedNewTaskState("task_ready");
    const preparationKey = newTaskPreparationKey(state);
    expect(preparationKey).toBeDefined();
    newTaskController.retain({
      preparationKey: preparationKey as string,
      snapshot: readyState.snapshot as TaskSnapshot,
    });

    await expect(fileBrowser?.listRoots()).resolves.toEqual([]);

    expect(request).toHaveBeenCalledWith(ATTACHMENT_LIST_ROOTS, { taskId: "task_ready" });
    expect(request).not.toHaveBeenCalledWith(TASK_ACQUIRE, expect.anything());
    expect(request).not.toHaveBeenCalledWith(TASK_RELEASE, expect.anything());
  });

  it("reuses a matching prepared Task even when the file picker was rendered from an older lease key", async () => {
    const request = vi.fn((method: string) => {
      if (method === ATTACHMENT_LIST_ROOTS) return Promise.resolve({ roots: [] });
      return Promise.reject(new Error(method));
    });
    const state = preparedNewTaskState();
    const readyState = preparedNewTaskState("task_ready");
    const newTaskController = new NewTaskController();
    newTaskController.retain({
      preparationKey: "superseded-render-key",
      snapshot: readyState.snapshot as TaskSnapshot,
    });
    const fileBrowser = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      newTaskController,
      state,
    }).newTask.fileBrowser;

    await expect(fileBrowser?.listRoots()).resolves.toEqual([]);

    expect(request).toHaveBeenCalledWith(ATTACHMENT_LIST_ROOTS, { taskId: "task_ready" });
    expect(request).not.toHaveBeenCalledWith(TASK_ACQUIRE, expect.anything());
    expect(request).not.toHaveBeenCalledWith(TASK_RELEASE, expect.anything());
  });

  it("does not release a pending prepared Task before validating that it matches the file picker", async () => {
    const request = vi.fn((method: string) => {
      if (method === ATTACHMENT_LIST_ROOTS) return Promise.resolve({ roots: [] });
      if (method === TASK_RELEASE) return Promise.resolve({ discarded: true });
      return Promise.reject(new Error(method));
    });
    const state = preparedNewTaskState("task_ready");
    state.snapshot = {
      ...state.snapshot as TaskSnapshot,
      task: { ...(state.snapshot as TaskSnapshot).task, agent_id: "opencode" },
    };
    const protocolTask = protocolTaskSnapshot("task_ready", "Prepared task");
    const mismatchedSnapshot = preparedNewTaskState("task_ready").snapshot as TaskSnapshot;
    mismatchedSnapshot.task = { ...mismatchedSnapshot.task, agent_id: "opencode" };
    const newTaskController = new NewTaskController();
    newTaskController.retain({
      preparationKey: "older-render-key",
      snapshot: mismatchedSnapshot,
    });
    const fileBrowser = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      newTaskController,
      pendingPreparedNewTask: () => Promise.resolve({ taskId: "task_ready" as never, task: protocolTask }),
      state,
    }).newTask.fileBrowser;

    await expect(fileBrowser?.listRoots()).resolves.toEqual([]);

    expect(request).toHaveBeenCalledWith(ATTACHMENT_LIST_ROOTS, { taskId: "task_ready" });
    expect(request).not.toHaveBeenCalledWith(TASK_RELEASE, expect.anything());
    expect(request).not.toHaveBeenCalledWith(TASK_ACQUIRE, expect.anything());
  });

  it("keeps an uploaded file when only the render-scoped browser callback is replaced", async () => {
    const uploaded = deferred<{ handleId: string; label: string }>();
    frontendShellState.files = {
      kind: "webUpload",
      upload: vi.fn(() => uploaded.promise),
    };
    const dispatch = vi.fn();
    const request = vi.fn((method: string) => {
      if (method === CLIENT_HEARTBEAT) return Promise.resolve({});
      if (method === ATTACHMENT_RELEASE) return Promise.resolve({ outcomes: [] });
      return Promise.reject(new Error(method));
    });
    const state = preparedNewTaskState("task_ready");
    const asyncOperations = new AsyncOperationOwner();
    const newTaskController = new NewTaskController();
    newTaskController.retain({
      preparationKey: newTaskPreparationKey(state) as string,
      snapshot: state.snapshot as TaskSnapshot,
    });
    const fileBrowser = callbacks({
      asyncOperations,
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      newTaskController,
      state,
    }).newTask.fileBrowser;
    const attaching = fileBrowser?.attachFiles?.([new File(["notes"], "notes.md")], {
      maxFiles: 1,
      onProgress: vi.fn(),
      signal: new AbortController().signal,
    });
    await Promise.resolve();
    asyncOperations.claim("new-task-file-browser", "replacement-render");
    uploaded.resolve({ handleId: "attachment-handle-1", label: "notes.md" });

    await expect(attaching).resolves.toBeUndefined();

    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:attachment:addAppServer",
      taskId: "task_ready",
    }));
    expect(request).not.toHaveBeenCalledWith(ATTACHMENT_RELEASE, expect.anything());
  });

  it("uploads a new-task image as binary and keeps only its handle in the draft", async () => {
    const upload = vi.fn(async () => ({ handleId: "image-handle-1", label: "pasted.png" }));
    frontendShellState.files = { kind: "webUpload", upload };
    const dispatch = vi.fn();
    const state = preparedNewTaskState("task_ready");

    const request = vi.fn();
    await callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).newTask.fileBrowser?.attachImage(
      new File([new Uint8Array([1, 2, 3])], "pasted.png", { type: "image/png" }),
    );

    expect(upload).toHaveBeenCalledWith(
      "task_ready",
      expect.any(File),
      expect.any(Function),
      expect.any(AbortSignal),
      { kind: "image", mimeType: "image/png" },
    );
    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:attachment:add",
      attachment: expect.objectContaining({
        kind: "image",
        label: "pasted.png",
        app_server_handle_id: "image-handle-1",
        preview_url: "data:image/png;base64,AQID",
      }),
    });
    expect(dispatch.mock.calls.at(-1)?.[0].attachment).not.toHaveProperty("payload");
  });

  it("uploads an active-task image as binary and keeps only its handle in the draft", async () => {
    const upload = vi.fn(async () => ({ handleId: "image-handle-1", label: "pasted.png" }));
    frontendShellState.files = { kind: "webUpload", upload };
    const dispatch = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    const request = vi.fn();
    await callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).task.fileBrowser?.attachImage(
      new File([new Uint8Array([1, 2, 3])], "pasted.png", { type: "image/png" }),
    );

    expect(upload).toHaveBeenCalledWith(
      "task_1",
      expect.any(File),
      expect.any(Function),
      expect.any(AbortSignal),
      { kind: "image", mimeType: "image/png" },
    );
    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:attachment:addAppServer",
      taskId: "task_1",
      attachment: expect.objectContaining({
        kind: "image",
        app_server_handle_id: "image-handle-1",
        preview_url: "data:image/png;base64,AQID",
      }),
    });
    expect(dispatch.mock.calls.at(-1)?.[0].attachment).not.toHaveProperty("payload");
  });

  it("acquires a fresh Prepared Task when client liveness expired in the file picker", async () => {
    const upload = vi.fn(async () => ({ handleId: "attachment-handle-1", label: "notes.md" }));
    frontendShellState.files = { kind: "webUpload", upload };
    const dispatch = vi.fn();
    const state = preparedNewTaskState("task_expired");
    const newTaskController = new NewTaskController();
    newTaskController.retain({
      preparationKey: newTaskPreparationKey(state) as string,
      snapshot: state.snapshot as TaskSnapshot,
    });
    const request = vi.fn((method: string) => {
      if (method === CLIENT_HEARTBEAT) {
        newTaskController.expireClientLease();
        return Promise.resolve({});
      }
      if (method === TASK_ACQUIRE) {
        return Promise.resolve({ task: protocolTaskSnapshot("task_recovered", "Recovered task") });
      }
      return Promise.reject(new Error(method));
    });
    const fileBrowser = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      newTaskController,
      state,
    }).newTask.fileBrowser;

    await expect(fileBrowser?.attachFiles?.([new File(["notes"], "notes.md")], {
      maxFiles: 1,
      onProgress: vi.fn(),
      signal: new AbortController().signal,
    })).resolves.toBeUndefined();

    expect(upload).toHaveBeenCalledWith("task_recovered", expect.any(File), expect.any(Function), expect.any(AbortSignal));
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:attachment:addAppServer",
      taskId: "task_recovered",
    }));
  });

  it("removes and releases a file from the prepared New Task composer", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({ outcomes: [] }));
    const state = preparedNewTaskState("task_ready");
    state.taskInputs.task_ready = {
      prompt: "Inspect this",
      context: [{
        local_id: "file-1",
        kind: "file",
        label: "notes.md",
        app_server_handle_id: "attachment-handle-1" as AttachmentHandleId,
      }],
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).newTask.removeAttachment("file-1");
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:attachment:remove",
      taskId: "task_ready",
      attachmentId: "file-1",
    });
    expect(request).toHaveBeenCalledWith(ATTACHMENT_RELEASE, {
      taskId: "task_ready",
      resources: [{ kind: "handle", id: "attachment-handle-1" }],
    });
  });

  it("discards a Task prepared after its file picker context was superseded", async () => {
    const created = deferred<{ task: ReturnType<typeof protocolTaskSnapshot> }>();
    const dispatch = vi.fn();
    const request = vi.fn((method: string) => {
      if (method === TASK_ACQUIRE) return created.promise;
      if (method === TASK_RELEASE) return Promise.resolve({ discarded: true });
      if (method === ATTACHMENT_LIST_ROOTS) return Promise.resolve({ roots: [] });
      return Promise.reject(new Error(method));
    });
    const state = preparedNewTaskState();
    const asyncOperations = new AsyncOperationOwner();
    const backendConnection = {
      request: request as unknown as BackendConnection["request"],
    };
    const fileBrowser = callbacks({
      asyncOperations,
      backendConnection,
      dispatch,
      state,
    }).newTask.fileBrowser;

    const listing = fileBrowser?.listRoots();
    await Promise.resolve();
    const nextState = {
      ...state,
      newTask: {
        ...state.newTask,
        selection: { ...state.newTask.selection, projectId: "project_2", agentId: "other-agent" },
      },
    };
    const newerFileBrowser = callbacks({ asyncOperations, backendConnection, dispatch, state: nextState }).newTask.fileBrowser;
    expect(newerFileBrowser?.ownerKey).not.toBe(fileBrowser?.ownerKey);
    created.resolve({ task: protocolTaskSnapshot("task_late", "Late task") });

    await expect(listing).rejects.toThrow("New Task context changed");
    expect(request).toHaveBeenCalledWith(TASK_RELEASE, { taskId: "task_late" });
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
      if (method === TASK_ACQUIRE) return created.promise;
      if (method === ATTACHMENT_LIST_ROOTS) return Promise.resolve({ roots: [] });
      return Promise.reject(new Error(method));
    });
    const state = preparedNewTaskState();
    state.newTask.prompt = "prompt captured before Task preparation";
    const fileBrowser = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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




  it("keeps New Task routed until Send is accepted, then adopts the accepted Task", async () => {
    const dispatch = vi.fn();
    const pendingSend = deferred<{
      task: ReturnType<typeof protocolTaskSnapshot>;
      turnId: string;
      userMessageId: string;
    }>();
    const request = vi.fn((method: string) => {
      if (method === TASK_ACQUIRE) return Promise.resolve({ task: protocolTaskSnapshot("task_1", "New task") });
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).newTask.submit();
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SEND, expect.objectContaining({ taskId: "task_1" }));
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:submit",
      taskId: "task_1",
      input: { prompt: "Build the thing", context: [] },
    }));
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "surface.openTask" }));

    pendingSend.resolve({
      turnId: "turn-1",
      userMessageId: "user-message",
      task: {
        ...protocolTaskSnapshot("task_1", "Accepted task", "visible"),
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
      type: "task:promoted",
      activate: true,
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
    }));
    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openTask",
      payload: { task_id: "task_1", title: "Accepted task" },
    });
  });

  it("settles an accepted first send before cancelling its active turn", async () => {
    const pendingSend = deferred<{
      task: ReturnType<typeof protocolTaskSnapshot>;
      turnId: string;
      userMessageId: string;
    }>();
    const dispatch = vi.fn();
    const request = vi.fn((method: string) => {
      if (method === TASK_ACQUIRE) return Promise.resolve({ task: protocolTaskSnapshot("task_1", "New task") });
      if (method === TASK_SEND) return pendingSend.promise;
      if (method === TASK_RELEASE) return Promise.reject(new Error("Task already has an active turn."));
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
    const newTaskController = new NewTaskController();
    const newTask = callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      newTaskController,
      state,
    }).newTask;

    newTask.submit();
    await settlePromises();
    const leaseB = newTaskController.claim({
      preparationKey: "context-b",
      taskId: "task_b" as never,
    });
    pendingSend.reject(new AppServerProtocolError({
      error: { code: "conflict", message: "Send A rejected", recoverable: true },
    }));
    await settlePromises();

    expect(newTaskController.currentLease()).toBe(leaseB);
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
        if (method === TASK_ACQUIRE) return { task: protocolPreparingTaskSnapshot("task_1", "New task") };
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
        backendConnection: { request: request as unknown as BackendConnection["request"] },
        dispatch,
        state,
      }).newTask.submit();
      for (let index = 0; index < 8; index += 1) await Promise.resolve();

      expect(request).toHaveBeenNthCalledWith(1, TASK_ACQUIRE, {
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

  it("keeps the prepared Task route and restores its draft when first send fails", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_ACQUIRE) return { task: protocolTaskSnapshot("task_1", "New task") };
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).newTask.submit();
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({ type: "taskInput:prompt", taskId: "task_1", prompt: "Build the thing" });
    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:sendError",
      taskId: "task_1",
      message: "send failed",
    });
    expect(dispatch).toHaveBeenCalledWith({ type: "submit:error", message: "send failed" });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "surface.openTask" }));
  });

  it("sends immediately when the prepared Task already owns the selected config options", async () => {
    vi.useFakeTimers();
    try {
      const dispatch = vi.fn();
      const request = vi.fn(async (method: string, _params?: unknown) => {
        if (method === TASK_ACQUIRE) {
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
        workspaceRoot: "/workspace",
        workspaceLabel: "workspace",
      };

      callbacks({
        backendConnection: { request: request as unknown as BackendConnection["request"] },
        dispatch,
        state,
      }).newTask.submit();
      await Promise.resolve();
      await Promise.resolve();

      expect(request).toHaveBeenNthCalledWith(1, TASK_ACQUIRE, {
        projectId: "project_1",
        agentId: "codex",
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).task.selectConfigOption("model", { type: "id", value: "gpt-5" });
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SET_CONFIG_OPTION, {
      taskId: "task_1",
      configId: "model",
      value: { type: "id", value: "gpt-5" },
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
      handleNotification: vi.fn(),
      request: request as unknown as BackendConnection["request"],
    };
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.snapshot.agent_config = editableConfigOptions();

    callbacks({ backendConnection, dispatch, state }).task.selectConfigOption("model", { type: "id", value: "gpt-5" });
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SET_CONFIG_OPTION, expect.objectContaining({
      taskId: "task_1",
      configId: "model",
      value: { type: "id", value: "gpt-5" },
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
        requested_value: { type: "id", value: "gpt-5" },
      },
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).task.selectConfigOption("model", { type: "id", value: "gpt-5" });
    await settlePromises();

    expect(request).not.toHaveBeenCalled();
    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:error",
      taskId: "task_1",
      message: "Configuration options are not currently editable.",
    });
  });

  it("refreshes an existing Task after a config mutation error clears backend pending state", async () => {
    vi.useFakeTimers();
    try {
      const dispatch = vi.fn();
      const request = vi.fn(async (method: string, _params?: unknown) => {
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
        backendConnection: { request: request as unknown as BackendConnection["request"] },
        dispatch,
        state,
      }).task.selectConfigOption("model", { type: "id", value: "gpt-5" });
      for (let index = 0; index < 8; index += 1) await Promise.resolve();

      const configRequest = request.mock.calls.find(([method]) => method === TASK_SET_CONFIG_OPTION);
      const mutationId = (configRequest?.[1] as { clientMutationId?: string } | undefined)?.clientMutationId;
      expect(request).toHaveBeenCalledWith(TASK_OPEN, { taskId: "task_1" });
      expect(dispatch).toHaveBeenCalledWith({
        type: "taskInput:configError",
        taskId: "task_1",
        mutationId,
        message: "Agent rejected the option",
        catalog: editableConfigOptions(),
      });
      expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: "snapshot", intent: "refresh" }));

      vi.advanceTimersByTime(10_000);
      expect(dispatch).toHaveBeenCalledWith({
        type: "taskInput:configError:clear",
        taskId: "task_1",
        mutationId,
      });
    } finally {
      vi.useRealTimers();
    }
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state: callbackState,
    }).task;

    task.selectConfigOption("model", { type: "id", value: "gpt-5" });
    dispatch({
      type: "taskInput:submit",
      taskId: "task_1",
      input: { prompt: "Send exactly once", context: [] },
    });
    configMutation.reject(new Error("Agent rejected the option"));
    await settlePromises();

    expect(renderedState.taskInputs.task_1.pending).toMatchObject({
      prompt: "Send exactly once",
      state: "sending",
    });
    expect(renderedState.taskInputs.task_1.configError?.message).toBe("Agent rejected the option");
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
    const state = createInitialState();
    state.settings.agentDetails = [{
      ...customSettingsAgent("codex"),
      auth_methods: [{ id: "codex-login", label: "Codex login", kind: "agent" }],
    }];
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
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

  it("keeps terminal authentication in Settings while user confirmation is pending", async () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.settings.agentDetails = [{
      ...customSettingsAgent("codex"),
      auth_methods: [{ id: "codex-login", label: "Codex login", kind: "terminal" }],
    }];
    const request = vi.fn(async (method: string) => method === AGENT_AUTHENTICATE
      ? { agentId: "codex", methodId: "codex-login", status: "awaiting_user" }
      : { generatedAt: "during-auth", agents: [] });

    const authenticated = await callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).settings.authenticateAgent("codex", "codex-login");

    expect(authenticated).toBe(false);
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({ type: "settings:error" }));
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

  it("does not render backend authentication error details", async () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.settings.agentDetails = [{ ...customSettingsAgent("codex"),
      auth_methods: [{ id: "api-key", label: "API Key", kind: "agent" }],
    }];

    callbacks({
      backendConnection: {
        request: vi.fn(async () => {
          throw new Error("internal error: CODEX_API_KEY is not set: { vendor metadata }");
        }) as unknown as BackendConnection["request"],
      },
      dispatch,
      state,
    }).settings.authenticateAgent("codex", "api-key");
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({
      type: "settings:error",
      message: "Authentication failed. Check the Agent's requirements and try again.",
    });
    expect(JSON.stringify(dispatch.mock.calls)).not.toContain("CODEX_API_KEY");
  });

  it("updates custom Agent metadata through BackendConnection when launch fields are unchanged", async () => {
    const request = vi.fn(async () => ({
      agentId: "custom.local",
      agents: protocolAgents(["codex", "custom.local"]),
    }));
    const state = createInitialState();
    state.settings.agentDetails = [customSettingsAgent("custom.local")];

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).task.sendPrompt();
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SEND, {
      taskId: "task_1",
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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


  it("cancels tasks through BackendConnection when available", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({ task: protocolTaskSnapshot("task_1", "Cancelled") }));
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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
      type: "taskInput:cancelError",
      taskId: "task_1",
      message: "App Server connection unavailable.",
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
      },
      dispatch,
      state,
    }).task.sendPrompt();
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:sendError",
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

  it("answers permissions through BackendConnection when available", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({}));
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({ backendConnection: { request: request as never }, dispatch, state }).task.respondToPermission(
      "server-request-1",
      "allow_once",
    );

    expect(dispatch).toHaveBeenNthCalledWith(1, {
      type: "permission:responding",
      requestId: "server-request-1",
    });
    expect(request).toHaveBeenCalledWith(PENDING_REQUEST_RESOLVE, {
      requestId: "server-request-1",
      resolution: { kind: "permission", optionId: "allow_once" },
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("waits for the authoritative Task snapshot after an accepted response", async () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({
      backendConnection: { request: vi.fn(async () => ({})) as never },
      dispatch,
      state,
    }).task.respondToPermission("server-request-1", "allow_once");
    await settlePromises();

    expect(dispatch).toHaveBeenNthCalledWith(1, {
      type: "permission:responding",
      requestId: "server-request-1",
    });
    expect(dispatch).toHaveBeenCalledTimes(1);
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("shows a recoverable permission error when BackendConnection request fails", async () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({
      backendConnection: {
        request: vi.fn(() => Promise.reject(new Error("connection closed"))) as never,
      },
      dispatch,
      state,
    }).task.respondToPermission("server-request-1", "allow_once");
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

  it("shows a recoverable permission error when BackendConnection request throws", () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    callbacks({
      backendConnection: {
        request: (() => {
          throw new Error("connection unavailable");
        }) as never,
      },
      dispatch,
      state,
    }).task.respondToPermission("server-request-1", "allow_once");

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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
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

  it("retains the New Task when opening an existing Task", async () => {
    const dispatch = vi.fn();
    const release = vi.fn();
    const attachmentResources = new ComposerAttachmentResourceOwner({ release });
    const request = vi.fn();
    const state = preparedNewTaskState("task_prepared");
    const navigation = callbacks({
      attachmentResources,
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).navigation;

    navigation.openTask("task_existing");
    await settlePromises();

    expect(request).not.toHaveBeenCalledWith(TASK_RELEASE, expect.anything());
    expect(release).not.toHaveBeenCalled();
    expect(dispatch).not.toHaveBeenCalledWith({ type: "taskInput:clear", taskId: "task_prepared" });
    expect(dispatch).not.toHaveBeenCalledWith({ type: "task:list:remove", taskId: "task_prepared" });
  });

  it("retains the New Task when opening Settings", async () => {
    const dispatch = vi.fn();
    const release = vi.fn();
    const attachmentResources = new ComposerAttachmentResourceOwner({ release });
    const request = vi.fn();
    const state = preparedNewTaskState("task_prepared");
    const navigation = callbacks({
      attachmentResources,
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).navigation;

    navigation.openSettings();
    await settlePromises();

    expect(request).not.toHaveBeenCalledWith(TASK_RELEASE, expect.anything());
    expect(release).not.toHaveBeenCalled();
    expect(dispatch).not.toHaveBeenCalledWith({ type: "taskInput:clear", taskId: "task_prepared" });
    expect(dispatch).not.toHaveBeenCalledWith({ type: "task:list:remove", taskId: "task_prepared" });
  });

  it("discards the prepared Task before adopting a Native Session", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_RELEASE) return { discarded: true };
      if (method === TASK_ADOPT_NATIVE_SESSION) {
        return { task: protocolTaskSnapshot("task_adopted", "Native Session") };
      }
      throw new Error(method);
    });
    const state = preparedNewTaskState("task_prepared");

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).navigation.openNativeSession({
      cwd: "/workspace",
      session_id: "native_1",
      title: "Native Session",
      updated_at: "2026-06-27T00:00:00Z",
    });
    await settlePromises();

    expect(request.mock.calls.filter(([method]) => method === TASK_RELEASE)).toEqual([
      [TASK_RELEASE, { taskId: "task_prepared" }],
    ]);
    expect(request.mock.calls.findIndex(([method]) => method === TASK_RELEASE))
      .toBeLessThan(request.mock.calls.findIndex(([method]) => method === TASK_ADOPT_NATIVE_SESSION));
    expect(request).toHaveBeenCalledWith(TASK_ADOPT_NATIVE_SESSION, expect.objectContaining({
      nativeSessionId: "native_1",
    }));
  });

  it("does not discard a prepared Task after its first send has started", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async (method: string) => {
      if (method === TASK_RELEASE) return { discarded: true };
      throw new Error(method);
    });
    const state = preparedNewTaskState("task_prepared");
    const newTaskController = new NewTaskController();
    const lease = newTaskController.claim({
      preparationKey: newTaskPreparationKey(state) as string,
      taskId: "task_prepared" as never,
    });
    newTaskController.protectSend(lease);

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      newTaskController,
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

    expect(request).not.toHaveBeenCalledWith(TASK_RELEASE, expect.anything());
  });

  it("does not let a stale prepared snapshot replace a newer ownership lease", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === TASK_RELEASE) return { discarded: true };
      throw new Error(method);
    });
    const state = preparedNewTaskState("task_stale");
    const newTaskController = new NewTaskController();
    const currentLease = newTaskController.claim({
      preparationKey: "newer-context",
      taskId: "task_newer" as never,
    });

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      newTaskController,
      state,
    }).navigation.openSettings();
    await settlePromises();

    expect(request).not.toHaveBeenCalledWith(TASK_RELEASE, expect.anything());
    expect(newTaskController.currentLease()).toBe(currentLease);
  });

  it("does not redirect when native-session adoption resolves after a newer navigation intent", async () => {
    const dispatch = vi.fn();
    let resolveRequest: ((value: { task: ReturnType<typeof protocolTaskSnapshot> }) => void) | undefined;
    const request = vi.fn(() => new Promise<{ task: ReturnType<typeof protocolTaskSnapshot> }>((resolve) => {
      resolveRequest = resolve;
    }));
    const asyncOperations = new AsyncOperationOwner();
    const state = createInitialState();
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project_1",
      workspaceRoot: "/workspace",
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      asyncOperations,
      dispatch,
      state,
    }).navigation.openNativeSession({
      cwd: "/workspace",
      session_id: "native_1",
      title: "Native Session",
      updated_at: "2026-06-27T00:00:00Z",
    });

    asyncOperations.beginNavigation("settings:default");
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
    const asyncOperations = new AsyncOperationOwner();
    const beginNavigation = vi.spyOn(asyncOperations, "beginNavigation");
    const state = preparedNewTaskState();
    const controllerCallbacks = callbacks({
      backendConnection: {
        request: vi.fn(() => adopted.promise) as unknown as BackendConnection["request"],
      },
      asyncOperations,
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

    expect(beginNavigation).toHaveBeenCalledTimes(2);
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
    const asyncOperations = new AsyncOperationOwner();
    let renderedState = preparedNewTaskState();
    const callbackState = renderedState;
    const dispatch = vi.fn((action) => {
      renderedState = appReducer(renderedState, action);
    });
    const controllerCallbacks = callbacks({
      backendConnection: {
        request: vi.fn(() => adopted.promise) as unknown as BackendConnection["request"],
      },
      asyncOperations,
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
    });

    adopted.resolve({ task: protocolTaskSnapshot("task_adopted", "Native Session") });
    await settlePromises();

    expect(renderedState.newTask.submitting).toBe(true);
    expect(renderedState.newTask.nativeSessions.adoptingSessionId).toBeUndefined();
  });

  it("does not surface a superseded native-session adoption error", async () => {
    const dispatch = vi.fn();
    let rejectRequest: ((error: Error) => void) | undefined;
    const request = vi.fn(() => new Promise((_, reject) => {
      rejectRequest = reject;
    }));
    const asyncOperations = new AsyncOperationOwner();
    const state = createInitialState();
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      agentLabel: "Codex",
      projectId: "project_1",
      workspaceRoot: "/workspace",
    };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      asyncOperations,
      dispatch,
      state,
    }).navigation.openNativeSession({
      cwd: "/workspace",
      session_id: "native_1",
      title: "Native Session",
      updated_at: "2026-06-27T00:00:00Z",
    });

    asyncOperations.beginNavigation("settings:default");
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
        };
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).navigation.archiveTask("task_1");
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SET_ARCHIVED, { taskId: "task_1", archived: true });
    expect(request).toHaveBeenCalledTimes(1);
    expect(dispatch).toHaveBeenCalledWith({ type: "selection:clear" });
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({ type: "tasks" }));
    expect(dispatch).not.toHaveBeenCalledWith({ type: "task:list:remove", taskId: "task_1" });
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
        };
      }
      throw new Error(method);
    });
    state.showArchived = true;
    state.activeTaskId = undefined;
    state.snapshot = undefined;
    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).navigation.restoreTask("task_1");
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SET_ARCHIVED, { taskId: "task_1", archived: false });
    expect(request).toHaveBeenCalledTimes(1);
    expect(dispatch).toHaveBeenCalledWith({ type: "archive:set", showArchived: false });
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({ type: "tasks" }));
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
        };
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.showArchived = true;
    state.activeTaskId = "task_1";
    state.snapshot = snapshot("task_1");
    state.taskInputs.task_1 = { prompt: "testing archived composer", context: [] };

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).navigation.restoreTask("task_1");
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SET_ARCHIVED, { taskId: "task_1", archived: false });
    expect(request).toHaveBeenCalledTimes(1);
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
        };
      }
      throw new Error(method);
    });
    const state = createInitialState();
    state.snapshot = snapshot("task_1");
    state.snapshot.task.project_id = "project_1";
    state.tasks = [state.snapshot.task];

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).navigation.archiveTask("task_1");
    await settlePromises();

    expect(dispatch).toHaveBeenCalledWith({ type: "selection:clear" });
    expect(request).toHaveBeenCalledTimes(1);
    expect(dispatch).not.toHaveBeenCalledWith({ type: "task:list:remove", taskId: "task_1" });
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).navigation.archiveTask("task_1");

    expect(postHostMessage).toHaveBeenCalledWith({
      type: "surface.openNewTask",
      payload: { project_id: "project_1" },
    });
  });

  it("toggles archive mode and reports an error when listing without BackendConnection", () => {
    const asyncOperations = new AsyncOperationOwner();
    const beginNavigation = vi.spyOn(asyncOperations, "beginNavigation");
    const dispatch = vi.fn();
    const state = createInitialState();

    callbacks({ asyncOperations, dispatch, state }).navigation.toggleArchived();

    expect(beginNavigation).toHaveBeenCalledWith("navigation:archived", true);
    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "archive:set", showArchived: true });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "surface.openArchive" }));
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "surface.openNewTask" }));
    expect(dispatch).toHaveBeenCalledWith({
      type: "tasks:error",
      message: "App Server connection unavailable.",
    });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "task.list" }));
    expectCalledBefore(beginNavigation, dispatch);
  });

  it("toggles archive mode through typed task list when BackendConnection is available", async () => {
    const asyncOperations = new AsyncOperationOwner();
    const beginNavigation = vi.spyOn(asyncOperations, "beginNavigation");
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({
      revision: 7,
      tasks: [protocolTaskSummary("task_archived", "Archived")],
      nextCursor: null,
    }));
    const state = createInitialState();

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      asyncOperations,
      dispatch,
      state,
    }).navigation.toggleArchived();
    await settlePromises();

    expect(beginNavigation).toHaveBeenCalledWith("navigation:archived", true);
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
    const asyncOperations = new AsyncOperationOwner();
    const beginNavigation = vi.spyOn(asyncOperations, "beginNavigation");
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({
      revision: 7,
      tasks: [protocolTaskSummary("task_archived_fresh", "Fresh Archived")],
    }));
    const state = createInitialState();
    state.taskListCache.archived = [snapshot("task_archived_cached").task];

    callbacks({
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      asyncOperations,
      dispatch,
      state,
    }).navigation.toggleArchived();
    await settlePromises();

    expect(beginNavigation).toHaveBeenCalledWith("navigation:archived", true);
    expect(dispatch).toHaveBeenCalledWith({ type: "archive:set", showArchived: true });
    expect(request).not.toHaveBeenCalled();
  });

  it("reports an error for config option changes without BackendConnection", () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    state.newTask.selection = {
      ...state.newTask.selection,
      agentId: "codex",
      projectId: "project-1",
    };

    callbacks({ dispatch, state }).newTask.selectConfigOption("model", { type: "id", value: "gpt-5" });

    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:configOptions:error",
      message: "Task session is not ready yet.",
    });
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "session.setConfigOption" }));
  });

  it("uses the prepared Task Native Session for config option changes", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({ task: protocolTaskSnapshot("task-prepared", "New task") }));
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).newTask.selectConfigOption("model", { type: "id", value: "gpt-5" });
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_SET_CONFIG_OPTION, {
      taskId: "task-prepared",
      configId: "model",
      value: { type: "id", value: "gpt-5" },
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      dispatch,
      state,
    }).newTask.selectConfigOption("model", { type: "id", value: "gpt-5" });
    await settlePromises();

    expect(request).toHaveBeenCalledWith(TASK_OPEN, { taskId: "task-prepared" });
    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:configOptions:error",
      message: "Unable to update Agent option.",
    });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: "snapshot", intent: "refresh" }));
    expect(renderedState.newTask.configOptionsError).toBe("Unable to update Agent option.");
  });

  it("loads earlier chat pages through BackendConnection", async () => {
    const dispatch = vi.fn();
    const asyncOperations = new AsyncOperationOwner();
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
      backendConnection: { request: request as unknown as BackendConnection["request"] },
      asyncOperations,
      dispatch,
      state,
    }).task.loadChatPage("cursor_1");
    await settlePromises();

    expect(requestGeneration).toEqual(expect.any(Number));
    expect(dispatch).toHaveBeenNthCalledWith(1, {
      type: "chatPage:start",
      taskId: "task_1",
      requestGeneration,
    });
    expect(request).toHaveBeenCalledWith(TASK_CHAT_PAGE, {
      taskId: "task_1",
      beforeCursor: "cursor_1",
      limit: 50,
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "chatPage:result",
      taskId: "task_1",
      requestGeneration,
      page: expect.objectContaining({
        task_id: "task_1",
        items: [expect.objectContaining({ message_id: "msg_1" })],
      }),
    });
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("subscribes to tool details until the disclosure releases its lease", async () => {
    const dispatch = vi.fn();
    const stopSubscription = vi.fn();
    const subscribeState = vi.fn((_scope, observer) => {
      observer.onSnapshot({
        kind: "toolDetail",
        taskId: "task_1",
        artifactId: "artifact_1",
        details: {
          locations: [{ path: "src/main.rs", line: 7 }],
          content: [{ kind: "text", text: "details" }],
          input: null,
          output: { exitCode: 0, success: true, fields: [] },
        },
      });
      return stopSubscription;
    });
    const state = createInitialState();
    state.snapshot = snapshot("task_1");

    const cleanup = callbacks({
      backendConnection: { subscribeState },
      dispatch,
      state,
    }).task.subscribeToolDetail("artifact_1");
    await settlePromises();

    expect(dispatch).toHaveBeenNthCalledWith(1, { type: "toolDetail:start", taskId: "task_1", artifactId: "artifact_1" });
    expect(subscribeState).toHaveBeenCalledWith(
      { kind: "toolDetail", taskId: "task_1", artifactId: "artifact_1" },
      expect.objectContaining({ onSnapshot: expect.any(Function) }),
    );
    expect(dispatch).toHaveBeenCalledWith({
      type: "toolDetail:result",
      taskId: "task_1",
      artifactId: "artifact_1",
      details: expect.objectContaining({
        locations: [{ path: "src/main.rs", line: 7 }],
        output: expect.objectContaining({ exit_code: 0 }),
      }),
    });
    cleanup();
    expect(stopSubscription).toHaveBeenCalledOnce();
    expect(postHostMessage).not.toHaveBeenCalled();
  });

  it("task callbacks no-op when there is no active snapshot", () => {
    const dispatch = vi.fn();
    const state = createInitialState();
    const task = callbacks({ dispatch, state }).task;

    task.cancel();
    task.loadChatPage("cursor_1");
    task.subscribeToolDetail("artifact_1");
    task.respondToPermission("permission_1", "allow_once");
    task.sendPrompt();

    expect(dispatch).not.toHaveBeenCalled();
    expect(postHostMessage).not.toHaveBeenCalled();
  });

});

function callbacks({
  attachmentResources,
  asyncOperations = new AsyncOperationOwner(),
  backendConnection,
  state = createInitialState(),
  dispatch = vi.fn(),
  newTaskStartAttempt = { current: undefined },
  pendingPreparedNewTask = vi.fn(() => undefined),
  newTaskController,
  requestNativeSessions = vi.fn(),
  setAgents = vi.fn(),
  setPreferences = vi.fn(),
}: Partial<Parameters<typeof createAppCallbacks>[0]> = {}) {
  state.appServerStateRootId ??= "state_root_1";
  const ownedNewTaskController = newTaskController ?? new NewTaskController();
  if (state.snapshot?.lifecycle === "new" && !ownedNewTaskController.currentTaskId()) {
    const preparationKey = newTaskPreparationKey(state);
    if (preparationKey) {
      ownedNewTaskController.claim({
        preparationKey,
        taskId: state.snapshot.task.task_id as never,
      });
    }
  }
  return createAppCallbacks({
    attachmentResources,
    asyncOperations,
    backendConnection,
    clientInstanceId: "test-client",
    createSnapshotRequestId: vi.fn(() => 91),
    dispatch,
    newTaskStartAttempt,
    pendingPreparedNewTask,
    newTaskController: ownedNewTaskController,
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
      lifecycle: "new",
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
    lifecycle: "visible",
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
    active_requests: [],
    send_capability: { state: "ready" },
    settings_summary: {
      agent_id: "codex",
      isolation: "local",
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
      kind: "select", current_value: { type: "id", value: "gpt" },
      values: [
        { id: "gpt", label: "GPT" },
        { id: "gpt-5", label: "GPT-5" },
      ],
    }],
  };
}

function protocolTaskSnapshot(
  taskId: string,
  title: string,
  lifecycle: ProtocolTaskSnapshot["lifecycle"] = "new",
): ProtocolTaskSnapshot {
  return {
    task: protocolTaskSummary(taskId, title),
    lifecycle,
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
    title: { value: title, source: "user" as const },
    status: "idle" as const,
    hasMessages: true,
    unread: false,
    updatedAt: "2026-05-22T00:00:00.000Z",
    lastActivity: "2026-05-22T00:00:00.000Z",
    workspaceAvailable: true,
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
