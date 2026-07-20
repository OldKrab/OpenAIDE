import { act, create } from "react-test-renderer";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  ATTACHMENT_RELEASE,
  type AttachmentHandleId,
  type BackendConnection,
} from "@openaide/app-server-client";
import { createInitialState, type AppState } from "../state/store";
import { useComposerAttachmentResources } from "./useComposerAttachmentResources";

describe("composer attachment resource lifecycle", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("releases the visible draft when navigation unmounts the Task surface", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({ outcomes: [] }));
    const state = stateWithDraft();
    let renderer: ReturnType<typeof create>;

    await act(async () => {
      renderer = create(<Probe dispatch={dispatch} request={request as unknown as BackendConnection["request"]} state={state} taskSurfaceMounted />);
    });
    await act(async () => {
      renderer.update(<Probe dispatch={dispatch} request={request as unknown as BackendConnection["request"]} state={state} taskSurfaceMounted={false} />);
    });

    expect(request).toHaveBeenCalledWith(ATTACHMENT_RELEASE, {
      taskId: "task-1",
      resources: [{ kind: "handle", id: "handle-1" }],
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "taskInput:attachment:remove",
      taskId: "task-1",
      attachmentId: "local-handle-1",
    });
  });

  it("retains the hidden New Task attachment across ordinary navigation", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({ outcomes: [] }));
    const state = stateWithDraft();
    let renderer: ReturnType<typeof create>;

    await act(async () => {
      renderer = create(<Probe
        dispatch={dispatch}
        newTaskId="task-1"
        request={request as unknown as BackendConnection["request"]}
        state={state}
        taskSurfaceMounted
      />);
    });
    await act(async () => {
      renderer.update(<Probe
        dispatch={dispatch}
        newTaskId="task-1"
        request={request as unknown as BackendConnection["request"]}
        state={{ ...state, snapshot: taskSnapshot("task-2") }}
        taskSurfaceMounted={false}
      />);
    });

    expect(request).not.toHaveBeenCalledWith(ATTACHMENT_RELEASE, expect.anything());
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({
      type: "taskInput:attachment:remove",
      taskId: "task-1",
    }));
  });

  it("releases the visible draft before the Frontend controller disconnects", async () => {
    const request = vi.fn(async () => ({ outcomes: [] }));
    let renderer!: ReturnType<typeof create>;
    await act(async () => {
      renderer = create(<Probe request={request as unknown as BackendConnection["request"]} state={stateWithDraft()} taskSurfaceMounted />);
    });

    await act(async () => renderer.unmount());

    expect(request).toHaveBeenCalledWith(ATTACHMENT_RELEASE, {
      taskId: "task-1",
      resources: [{ kind: "handle", id: "handle-1" }],
    });
  });

  it("does not release an in-flight send handle when its controller unmounts", async () => {
    const request = vi.fn();
    const state = stateWithDraft();
    state.taskInputs["task-1"].pending = {
      prompt: "Draft",
      context: state.taskInputs["task-1"].context,
      state: "sending",
    };
    let renderer!: ReturnType<typeof create>;
    await act(async () => {
      renderer = create(<Probe request={request as unknown as BackendConnection["request"]} state={state} taskSurfaceMounted />);
    });

    await act(async () => renderer.unmount());

    expect(request).not.toHaveBeenCalledWith(ATTACHMENT_RELEASE, expect.anything());
  });

  it("does not release a previous state root's resolver handle through the new root", async () => {
    const request = vi.fn(async () => ({ outcomes: [] }));
    const rootOne = stateWithDraft();
    const rootTwo = createInitialState();
    rootTwo.appServerStateRootId = "root-b";
    let renderer!: ReturnType<typeof create>;

    await act(async () => {
      renderer = create(<Probe
        request={request as unknown as BackendConnection["request"]}
        state={rootOne}
        taskSurfaceMounted
      />);
    });
    await act(async () => {
      renderer.update(<Probe
        request={request as unknown as BackendConnection["request"]}
        state={rootTwo}
        taskSurfaceMounted
      />);
    });

    expect(request).not.toHaveBeenCalledWith(ATTACHMENT_RELEASE, expect.anything());
    await act(async () => renderer.unmount());
  });

  it("keeps inactive Task drafts owned across A to B to A navigation", async () => {
    const request = vi.fn(async () => ({ outcomes: [] }));
    const stateA = stateWithDraft();
    stateA.taskInputs["task-2"] = {
      prompt: "Second draft",
      context: [{
        kind: "file",
        label: "second.md",
        local_id: "local-handle-2",
        app_server_handle_id: "handle-2" as AttachmentHandleId,
      }],
    };
    let renderer!: ReturnType<typeof create>;
    await act(async () => {
      renderer = create(<Probe request={request as unknown as BackendConnection["request"]} state={stateA} taskSurfaceMounted />);
    });
    await act(async () => {
      renderer.update(<Probe
        request={request as unknown as BackendConnection["request"]}
        state={{ ...stateA, snapshot: taskSnapshot("task-2") }}
        taskSurfaceMounted
      />);
    });
    await act(async () => {
      renderer.update(<Probe request={request as unknown as BackendConnection["request"]} state={stateA} taskSurfaceMounted />);
    });

    expect(request).not.toHaveBeenCalledWith(ATTACHMENT_RELEASE, expect.anything());

    await act(async () => renderer.unmount());
    expect(request).toHaveBeenCalledWith(ATTACHMENT_RELEASE, {
      taskId: "task-1",
      resources: [{ kind: "handle", id: "handle-1" }],
    });
    expect(request).toHaveBeenCalledWith(ATTACHMENT_RELEASE, {
      taskId: "task-2",
      resources: [{ kind: "handle", id: "handle-2" }],
    });
  });
});

function Probe({
  dispatch,
  newTaskId,
  request,
  state,
  taskSurfaceMounted,
}: {
  dispatch?: Parameters<typeof useComposerAttachmentResources>[0]["dispatch"];
  newTaskId?: string;
  request: BackendConnection["request"];
  state: AppState;
  taskSurfaceMounted: boolean;
}) {
  useComposerAttachmentResources({
    backendConnection: { request },
    clientInstanceId: "client-1",
    dispatch,
    newTaskId,
    state,
    taskSurfaceMounted,
  });
  return null;
}

function stateWithDraft() {
  const state = createInitialState();
  state.appServerStateRootId = "root-a";
  state.snapshot = taskSnapshot("task-1");
  state.taskInputs["task-1"] = {
    prompt: "Draft",
    context: [{
      kind: "file",
      label: "notes.md",
      local_id: "local-handle-1",
      app_server_handle_id: "handle-1" as AttachmentHandleId,
    }],
  };
  return state;
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

function taskSnapshot(taskId: string): NonNullable<AppState["snapshot"]> {
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
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
      last_activity: "2026-01-01T00:00:00Z",
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
    settings_summary: { agent_id: "codex", isolation: "local" },
    revision: 1,
  };
}
