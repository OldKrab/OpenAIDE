import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatMessage, TaskSnapshot } from "@openaide/app-shell-contracts";

describe("TaskView timeline presentation", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    vi.useFakeTimers();
    vi.stubGlobal("window", {
      acquireVsCodeApi: undefined,
      clearTimeout: globalThis.clearTimeout,
      setTimeout: globalThis.setTimeout,
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("does not show a stale caret on streaming text superseded by later timeline rows", async () => {
    const { TaskView } = await import("./TaskView");
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<TaskView {...taskViewProps(snapshotWithAuthoritativeTail(true))} />);
    });

    const renderedTask = JSON.stringify(tree.toJSON());
    expect(renderedTask).toContain("Earlier response");
    expect(renderedTask).toContain("Steered follow-up");
    expect(renderedTask).toContain("npm test");
    expect(renderedTask).toContain("Latest update");
    expect(renderedTask).not.toContain("chat-streaming-caret");
  });

  it("renders a task-open failure as a settled recoverable state", async () => {
    const { TaskLoadingView } = await import("./TaskView");
    const retry = vi.fn();
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<TaskLoadingView error="Connection closed." onRetry={retry} />);
    });

    expect(tree.root.findByType("section").props["aria-label"]).toBe("Unable to open task");
    expect(tree.root.findAllByProps({ className: "working-status-dots" })).toHaveLength(0);
    expect(JSON.stringify(tree.toJSON())).toContain("Connection closed.");

    act(() => {
      tree.root.findByType("button").props.onClick();
    });
    expect(retry).toHaveBeenCalledOnce();
  });

  it("announces each completed history generation only once", async () => {
    const { TaskView } = await import("./TaskView");
    const updated = snapshotWithAuthoritativeTail(true);
    updated.history_sync = { state: "updated", generation: 3 };
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<TaskView {...taskViewProps(updated)} />);
    });
    expect(JSON.stringify(tree.toJSON())).toContain("History updated");

    act(() => {
      vi.advanceTimersByTime(2_000);
    });
    expect(JSON.stringify(tree.toJSON())).not.toContain("History updated");

    const stale = snapshotWithAuthoritativeTail(true);
    stale.history_sync = { state: "idle", generation: 0 };
    act(() => {
      tree.update(<TaskView {...taskViewProps(stale)} />);
    });
    act(() => {
      tree.update(<TaskView {...taskViewProps(updated)} />);
    });

    expect(JSON.stringify(tree.toJSON())).not.toContain("History updated");
  });

  it("locks Agent configuration while the Task subscription is unavailable", async () => {
    const { TaskView } = await import("./TaskView");
    const snapshot = snapshotWithAuthoritativeTail(true);
    snapshot.agent_config = {
      agent_id: "codex",
      status: "ready",
      options: [{
        current_value: "off",
        id: "fast-mode",
        label: "Fast mode",
        values: [
          { id: "off", label: "Off" },
          { id: "on", label: "On" },
        ],
      }],
    };
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<TaskView {...taskViewProps(snapshot)} backendReady={false} />);
    });

    const configControl = tree.root.find((node) =>
      typeof node.props.className === "string"
      && node.props.className.split(/\s+/).includes("composer-config-control"));
    expect(configControl.props.disabled).toBe(true);
  });

  it("keeps a draft editable while the Task subscription reconnects", async () => {
    const { TaskView } = await import("./TaskView");
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(
        <TaskView
          {...taskViewProps(snapshotWithAuthoritativeTail(true))}
          backendConnectionState={{ status: "reconnecting", message: "Connection closed." }}
          backendReady={false}
          taskInput={{ prompt: "Keep this draft", context: [] }}
        />,
      );
    });

    const rendered = JSON.stringify(tree.toJSON());
    expect(rendered).toContain("Reconnecting to App Server.");
    expect(rendered).toContain("Connection closed.");
    const editor = tree.root.findByProps({ role: "textbox", "aria-label": "Message" });
    expect(editor.props.contentEditable).toBe(true);
    expect(editor.props["aria-placeholder"]).toBe("Reconnecting. Draft is saved here.");
    expect(tree.root.findByProps({ "aria-label": "Send message" }).props.disabled).toBe(true);
  });

  it("shows cached task history with an in-place retry when task refresh fails", async () => {
    const { TaskView } = await import("./TaskView");
    const retry = vi.fn();
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(
        <TaskView
          {...taskViewProps(snapshotWithAuthoritativeTail(true))}
          backendConnectionState={{ status: "unavailable", message: "Connection closed." }}
          backendReady={false}
          onRetryConnection={retry}
          taskInput={{ prompt: "Keep this draft", context: [] }}
        />,
      );
    });

    const rendered = JSON.stringify(tree.toJSON());
    expect(rendered).toContain("Unable to refresh task.");
    expect(rendered).toContain("Earlier response");
    expect(rendered).toContain("Connection closed.");
    expect(tree.root.findByProps({ role: "textbox", "aria-label": "Message" }).props.contentEditable).toBe(true);
    const retryButton = tree.root.find((node) => node.type === "button" && node.children.includes("Retry"));
    act(() => retryButton.props.onClick());
    expect(retry).toHaveBeenCalledOnce();
  });
});

function taskViewProps(snapshot: TaskSnapshot) {
  return {
    appServerPermissionRequests: {},
    backendReady: true,
    chatPageState: undefined,
    dispatch: vi.fn(),
    onCancel: vi.fn(),
    onLoadChatPage: vi.fn(),
    onLoadToolDetail: vi.fn(),
    onPermissionRespond: vi.fn(),
    onRevealAttachment: vi.fn(),
    onRemoveAttachment: vi.fn(),
    onSendPrompt: vi.fn(),
    onSelectConfigOption: vi.fn(),
    permissionResponses: {},
    snapshot,
    taskInput: { prompt: "", context: [] },
    toolDetails: {},
    submitShortcut: "mod_enter" as const,
  };
}

function snapshotWithAuthoritativeTail(includeTail: boolean): TaskSnapshot {
  const revision = includeTail ? 4 : 1;
  const items: ChatMessage[] = [agentText("agent-streaming", "Earlier response", true)];
  if (includeTail) {
    items.push(
      {
        cursor: "user-steer",
        identity: "user-steer",
        message_id: "user-steer",
        message_type: "user",
        message: {
          kind: "user",
          id: "user-steer",
          text: "Steered follow-up",
          created_at: "2026-07-12T00:00:01Z",
        },
      },
      {
        cursor: "tool-running",
        identity: "tool-running",
        message_id: "tool-running",
        message_type: "activity",
        message: {
          kind: "activity",
          id: "tool-running",
          title: "Run tests",
          status: "running",
          created_at: "2026-07-12T00:00:02Z",
          collapsed: true,
          steps: [{ kind: "tool", name: "execute", status: "running", input_summary: "npm test" }],
        },
      },
      agentText("agent-later", "Latest update", false),
    );
  }

  return {
    task: {
      task_id: "task-1",
      title: "Task",
      status: "active",
      task_version: revision,
      message_history_version: revision,
      has_messages: true,
      unread: false,
      created_at: "2026-07-12T00:00:00Z",
      updated_at: "2026-07-12T00:00:03Z",
      last_activity: "2026-07-12T00:00:03Z",
      agent_id: "codex",
      agent_name: "Codex",
      isolation: "local",
      workspace_root: "/workspace",
    },
    history_sync: { state: "idle", generation: 0 },
    chat: {
      task_id: "task-1",
      items,
      has_before: false,
      has_messages: true,
      total_count: items.length,
      version: revision,
    },
    permissions: [],
    send_capability: { state: "ready", attachment_only: true },
    settings_summary: { agent_id: "codex", isolation: "local" },
    revision,
  };
}

function agentText(messageId: string, text: string, streaming: boolean): ChatMessage {
  return {
    cursor: messageId,
    identity: messageId,
    message_id: messageId,
    message_type: "agent_text",
    message: {
      kind: "agent_text",
      id: messageId,
      text,
      created_at: "2026-07-12T00:00:00Z",
      streaming,
    },
  };
}
