import { describe, expect, it, vi } from "vitest";
import {
  ATTACHMENT_RELEASE,
  type AttachmentCandidateId,
  type AttachmentHandleId,
  type BackendConnection,
} from "@openaide/app-server-client";
import {
  attachmentCandidateResource,
  attachmentHandleResource,
  composerAttachmentResourceFrame,
  ComposerAttachmentResourceOwner,
  releaseAttachmentResources,
  type ComposerAttachmentResourceFrame,
} from "./attachmentResources";
import { createInitialState, type AppState } from "../state/store";

describe("composer attachment resource ownership", () => {
  it("releases mixed resources in caller order through the attachment release contract", () => {
    const request = vi.fn(async () => ({ outcomes: [] }));

    releaseAttachmentResources(
      { request: request as unknown as BackendConnection["request"] },
      "task-1",
      [
        attachmentHandleResource("handle-1" as AttachmentHandleId),
        attachmentCandidateResource("candidate-1" as AttachmentCandidateId),
      ],
    );

    expect(request).toHaveBeenCalledWith(ATTACHMENT_RELEASE, {
      taskId: "task-1",
      resources: [
        { kind: "handle", id: "handle-1" },
        { kind: "candidate", id: "candidate-1" },
      ],
    });
  });

  it("releases a draft handle when its Task composer is no longer mounted", () => {
    const release = vi.fn();
    const owner = new ComposerAttachmentResourceOwner({ release });

    owner.reconcile(frame({ retained: [resource("task-1", "handle-1")] }));
    owner.reconcile(frame());

    expect(release).toHaveBeenCalledWith("task-1", ["handle-1"]);
  });

  it("forgets consumed handles instead of releasing them after send acceptance", () => {
    const release = vi.fn();
    const owner = new ComposerAttachmentResourceOwner({ release });

    owner.reconcile(frame({
      acceptedUserMessageIds: new Map([["task-1", "message-before-send"]]),
      retained: [resource("task-1", "handle-1")],
    }));
    owner.reconcile(frame({
      acceptedUserMessageIds: new Map([["task-1", "accepted-message"]]),
    }));

    expect(release).not.toHaveBeenCalled();
  });

  it("releases an explicitly removed row once even when render cleanup follows", () => {
    const release = vi.fn();
    const owner = new ComposerAttachmentResourceOwner({ release });
    const attachment = resource("task-1", "handle-1");

    owner.reconcile(frame({ retained: [attachment] }));
    owner.release(attachment);
    owner.reconcile(frame());

    expect(release).toHaveBeenCalledTimes(1);
    expect(release).toHaveBeenCalledWith("task-1", ["handle-1"]);
  });

  it("batches cleanup when an authoritative rejection abandons multiple rows", () => {
    const release = vi.fn();
    const owner = new ComposerAttachmentResourceOwner({ release });
    const attachments = [
      resource("task-1", "handle-1"),
      resource("task-1", "handle-2"),
    ];
    owner.reconcile(frame({ retained: attachments, mountedTaskId: "task-1" }));

    owner.releaseAll(attachments);
    owner.reconcile(frame());

    expect(release).toHaveBeenCalledTimes(1);
    expect(release).toHaveBeenCalledWith("task-1", ["handle-1", "handle-2"]);
  });

  it("keeps an ambiguous send handle owned until same-client recovery is resolved", () => {
    const release = vi.fn();
    let recoveryPending = true;
    const owner = new ComposerAttachmentResourceOwner({
      isProtected: () => recoveryPending,
      release,
    });

    owner.reconcile(frame({ retained: [resource("task-1", "handle-1")] }));
    owner.reconcile(frame());
    expect(release).not.toHaveBeenCalled();

    recoveryPending = false;
    owner.reconcile(frame());
    expect(release).toHaveBeenCalledWith("task-1", ["handle-1"]);
  });

  it("does not let row removal release a handle owned by ambiguous send recovery", () => {
    const release = vi.fn();
    let recoveryPending = true;
    const owner = new ComposerAttachmentResourceOwner({
      isProtected: () => recoveryPending,
      release,
    });
    const attachment = resource("task-1", "handle-1");
    owner.reconcile(frame({ retained: [attachment], mountedTaskId: "task-1" }));

    owner.release(attachment);
    expect(release).not.toHaveBeenCalled();

    recoveryPending = false;
    owner.reconcile(frame());
    expect(release).toHaveBeenCalledWith("task-1", ["handle-1"]);
  });

  it("releases live draft handles when the Frontend controller unmounts", () => {
    const release = vi.fn();
    const owner = new ComposerAttachmentResourceOwner({ release });
    const current = frame({ retained: [resource("task-1", "handle-1")] });

    owner.reconcile(current);
    owner.dispose(current);

    expect(release).toHaveBeenCalledWith("task-1", ["handle-1"]);
  });

  it("observes the mounted prepared-Task composer and protects its pending send", () => {
    const state = createInitialState();
    state.snapshot = taskSnapshot("task-1", false);
    state.taskInputs["task-1"] = {
      prompt: "Draft",
      context: [attachment("handle-1")],
      pending: {
        prompt: "Draft",
        context: [attachment("handle-1")],
        state: "sending",
      },
    };

    const observed = composerAttachmentResourceFrame(state, true);

    expect(observed.retained).toEqual([resource("task-1", "handle-1")]);
    expect(observed.protected).toEqual(new Set(["task-1\u0000handle-1"]));
  });

  it("releases a handle whose selection response arrives after its composer unmounted", () => {
    const release = vi.fn();
    const owner = new ComposerAttachmentResourceOwner({ release });
    owner.reconcile(frame());

    owner.adopt(resource("task-1", "handle-1"));

    expect(release).toHaveBeenCalledWith("task-1", ["handle-1"]);
  });

  it("forgets old-root handles without releasing them into a replacement root", () => {
    const release = vi.fn();
    const owner = new ComposerAttachmentResourceOwner({ release });
    const oldResource = resource("task-1", "handle-1");
    owner.reconcile(frame({ retained: [oldResource], mountedTaskId: "task-1" }));
    const oldAdoption = owner.beginAdoption("task-1");

    owner.replaceStateRoot();
    owner.reconcile(frame({ mountedTaskId: "task-1" }));
    expect(owner.adopt(oldResource, oldAdoption)).toBe(false);
    expect(release).not.toHaveBeenCalled();

    owner.adopt(oldResource);
    owner.reconcile(frame());
    expect(release).toHaveBeenCalledOnce();
    expect(release).toHaveBeenCalledWith("task-1", ["handle-1"]);
  });

  it("keeps a newly selected handle owned while its composer remains mounted", () => {
    const release = vi.fn();
    const owner = new ComposerAttachmentResourceOwner({ release });
    const attachment = resource("task-1", "handle-1");
    owner.reconcile(frame({ mountedTaskId: "task-1" }));

    owner.adopt(attachment);
    owner.reconcile(frame({ retained: [attachment], mountedTaskId: "task-1" }));

    expect(release).not.toHaveBeenCalled();
  });

  it("claims a just-prepared Task before the first attachment selection completes", () => {
    const release = vi.fn();
    const owner = new ComposerAttachmentResourceOwner({ release });
    owner.reconcile(frame({ taskSurfaceMounted: true }));

    owner.claimNewTaskController("task-1");
    owner.adopt(resource("task-1", "handle-1"));

    expect(release).not.toHaveBeenCalled();
  });

  it("preserves inactive Task drafts across A to B to A navigation", () => {
    const release = vi.fn();
    const owner = new ComposerAttachmentResourceOwner({ release });
    const state = createInitialState();
    state.taskInputs["task-a"] = { prompt: "A", context: [attachment("handle-a")] };
    state.taskInputs["task-b"] = { prompt: "B", context: [attachment("handle-b")] };

    state.snapshot = taskSnapshot("task-a", true);
    owner.reconcile(composerAttachmentResourceFrame(state, true));
    state.snapshot = taskSnapshot("task-b", true);
    owner.reconcile(composerAttachmentResourceFrame(state, true));
    state.snapshot = taskSnapshot("task-a", true);
    owner.reconcile(composerAttachmentResourceFrame(state, true));

    expect(release).not.toHaveBeenCalled();

    delete state.taskInputs["task-a"];
    owner.reconcile(composerAttachmentResourceFrame(state, true));

    expect(release).toHaveBeenCalledWith("task-a", ["handle-a"]);
    expect(release).not.toHaveBeenCalledWith("task-b", ["handle-b"]);
  });

  it("rejects and releases a late selection response after A to B to A navigation", () => {
    const release = vi.fn();
    const owner = new ComposerAttachmentResourceOwner({ release });
    owner.reconcile(frame({ mountedTaskId: "task-a", taskSurfaceMounted: true }));
    const adoption = owner.beginAdoption("task-a");
    owner.reconcile(frame({ mountedTaskId: "task-b", taskSurfaceMounted: true }));
    owner.reconcile(frame({ mountedTaskId: "task-a", taskSurfaceMounted: true }));

    const adopted = owner.adopt(resource("task-a", "late-handle"), adoption);

    expect(adopted).toBe(false);
    expect(release).toHaveBeenCalledWith("task-a", ["late-handle"]);
  });

  it("rejects selection responses after send locks the mounted composer", () => {
    const release = vi.fn();
    const owner = new ComposerAttachmentResourceOwner({ release });
    owner.reconcile(frame({ mountedTaskId: "task-a", taskSurfaceMounted: true }));
    const adoption = owner.beginAdoption("task-a");

    owner.lockAdoptions();
    owner.reconcile(frame({ mountedTaskId: "task-a", taskSurfaceMounted: true }));
    const adopted = owner.adopt(resource("task-a", "late-handle"), adoption);

    expect(adopted).toBe(false);
    expect(release).toHaveBeenCalledWith("task-a", ["late-handle"]);
  });
});

function frame(overrides: Partial<ComposerAttachmentResourceFrame> = {}): ComposerAttachmentResourceFrame {
  return {
    acceptedUserMessageIds: new Map(),
    acceptsAdoptions: true,
    retained: [],
    mountedTaskId: undefined,
    protected: new Set(),
    taskSurfaceMounted: false,
    ...overrides,
  };
}

function resource(taskId: string, handleId: string) {
  return { taskId, handleId: handleId as AttachmentHandleId };
}

function attachment(handleId: string) {
  return {
    kind: "file" as const,
    label: "notes.md",
    local_id: `local-${handleId}`,
    app_server_handle_id: handleId as AttachmentHandleId,
  };
}

function taskSnapshot(taskId: string, hasMessages: boolean): NonNullable<AppState["snapshot"]> {
  return {
    lifecycle: hasMessages ? "visible" : "new",
    task: {
      task_id: taskId,
      title: "Task",
      status: "inactive",
      task_version: 1,
      message_history_version: 1,
      has_messages: hasMessages,
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
      has_messages: hasMessages,
      total_count: 0,
      version: 1,
    },
    permissions: [],
    send_capability: { state: "ready", attachment_only: true },
    settings_summary: { agent_id: "codex", isolation: "local", config_options: {} },
    revision: 1,
  };
}
