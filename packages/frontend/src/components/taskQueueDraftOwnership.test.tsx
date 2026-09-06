import { act, useReducer } from "react";
import { create, type ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import { ATTACHMENT_RELEASE, TASK_QUEUE_TAKE, type TaskQueueTakeResult } from "@openaide/app-server-client";
import { takeTaskQueueMessageIntent } from "../intents/taskMutationIntents";
import { appReducer, type AppAction } from "../state/appReducer";
import { createInitialState } from "../state/store";
import { useComposerAttachmentResources } from "./useComposerAttachmentResources";

beforeEach(() => vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true));
afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers(); });

describe("Composer attachment lifetime", () => {
  it("retains an existing Task's draft files while the user visits Settings", async () => {
    let state = createInitialState();
    state.snapshot = taskSnapshot();
    state.taskInputs["task-a"] = {
      prompt: "Inspect this file",
      context: [{
        kind: "file", label: "notes.txt", local_id: "file-a",
        app_server_handle_id: "handle-a" as never,
      }],
    };
    const request = vi.fn().mockResolvedValue({ outcomes: [] });
    const dispatch = (action: AppAction) => { state = appReducer(state, action); };
    function Harness({ mounted }: { mounted: boolean }) {
      useComposerAttachmentResources({
        backendConnection: { request }, clientInstanceId: "client-a", dispatch,
        state, taskSurfaceMounted: mounted,
      });
      return null;
    }
    let renderer!: ReactTestRenderer;
    try {
      await act(async () => { renderer = create(<Harness mounted />); });
      await act(async () => { renderer.update(<Harness mounted={false} />); });
      expect(state.taskInputs["task-a"].context).toHaveLength(1);
      expect(request).not.toHaveBeenCalled();
      await act(async () => { renderer.update(<Harness mounted />); });
      expect(state.taskInputs["task-a"].context[0].app_server_handle_id).toBe("handle-a");
    } finally {
      await act(async () => renderer?.unmount());
    }
  });

  it.each([false, true])("retains files extracted from the queue through collapse (navigated: %s)", async (navigated) => {
    vi.useFakeTimers();
    const initial = createInitialState();
    initial.snapshot = taskSnapshot();
    initial.snapshot.message_queue = {
      revision: 1,
      items: [{ queued_message_id: "queued-a", text: "Review notes", created_at: "now" }],
    };
    let state = initial;
    let resolveTake!: (result: TaskQueueTakeResult) => void;
    const remoteTake = new Promise<TaskQueueTakeResult>((resolve) => { resolveTake = resolve; });
    const request = vi.fn((method: string) => method === TASK_QUEUE_TAKE
      ? remoteTake : Promise.resolve({ outcomes: [] }));
    let take!: () => Promise<void>;
    function Harness({ mounted }: { mounted: boolean }) {
      const [current, dispatch] = useReducer(appReducer, initial);
      state = current;
      const resources = useComposerAttachmentResources({
        backendConnection: { request: request as never }, clientInstanceId: "client-a", dispatch,
        state: current, taskSurfaceMounted: mounted,
      });
      take = () => takeTaskQueueMessageIntent({
        attachmentResources: resources, backendConnection: { request: request as never },
        clientInstanceId: "client-a", createSnapshotRequestId: () => 1,
        dispatch, postHostMessage: vi.fn(), stateRootId: "root-a",
      }, current.snapshot, current.taskInputs["task-a"] ?? { prompt: "", context: [] }, "queued-a");
      return null;
    }
    let renderer!: ReactTestRenderer;
    try {
      await act(async () => { renderer = create(<Harness mounted />); });
      let pendingTake!: Promise<void>;
      await act(async () => { pendingTake = take(); });
      if (navigated) await act(async () => { renderer.update(<Harness mounted={false} />); });
      await act(async () => { resolveTake(queueTakeResult()); await pendingTake; });
      expect(state.taskInputs["task-a"].queueTake?.stage).toBe("collapsing");
      expect(request.mock.calls.some(([method]) => method === ATTACHMENT_RELEASE)).toBe(false);
      await act(async () => { await vi.advanceTimersByTimeAsync(180); });
      expect(state.taskInputs["task-a"].context[0]?.app_server_handle_id).toBe("handle-taken");
      expect(state.taskInputs["task-a"].prompt).toBe("Review notes");
      expect(request.mock.calls.some(([method]) => method === ATTACHMENT_RELEASE)).toBe(false);
    } finally {
      await act(async () => renderer?.unmount());
    }
  });
});

function queueTakeResult(): TaskQueueTakeResult {
  return {
    message: { text: "Review notes", attachments: [{ handleId: "handle-taken" as never, label: "notes.txt" }] },
    task: {
      task: {
        taskId: "task-a" as never, projectId: "project-a" as never, agentId: "codex" as never,
        lifecycle: "open", title: { value: "Task", source: "user" }, status: "idle",
        updatedAt: "2026-07-12T00:00:00Z", lastActivity: "2026-07-12T00:00:00Z",
        unread: false, hasMessages: true, workspaceAvailable: true,
      },
      lifecycle: "open", revision: 2, permissionPolicy: "askEveryTime",
      preparation: { kind: "ready" }, agentConfig: { state: "ready", options: [] },
      agentCommands: { state: "ready", commands: [] }, sendCapability: { state: "ready" },
      messageQueue: { revision: 2, items: [] }, historySync: { state: "idle", generation: 0 },
      subagents: { totalCount: 0, runningCount: 0, attentionCount: 0, available: true },
      chat: { items: [], hasMessages: true }, pendingRequests: [],
    },
  };
}

function taskSnapshot(): TaskSnapshot {
  return {
    lifecycle: "open", permission_policy: "ask_every_time", revision: 1,
    task: {
      task_id: "task-a", project_id: "project-a", title: "Task", status: "inactive",
      task_version: 1, message_history_version: 1, has_messages: true, unread: false, pinned: false,
      created_at: "2026-07-12T00:00:00Z", updated_at: "2026-07-12T00:00:00Z",
      last_activity: "2026-07-12T00:00:00Z", agent_id: "codex", agent_name: "Codex",
      isolation: "local", workspace_root: "/workspace",
    },
    chat: {
      task_id: "task-a", items: [], has_before: false, has_messages: true, total_count: 0, version: 1,
    },
    active_requests: [], message_queue: { revision: 0, items: [] },
    settings_summary: { agent_id: "codex", isolation: "local" },
    send_capability: { state: "ready" }, history_sync: { state: "idle", generation: 0 },
  };
}
