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

  it("keeps live presentation on the latest Agent message across later non-Agent rows", async () => {
    const { TaskView } = await import("./TaskView");
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<TaskView {...taskViewProps(snapshotWithAuthoritativeTail(true))} />);
    });
    expect(JSON.stringify(tree.toJSON())).not.toContain("chat-streaming-caret");

    const updated = snapshotWithAuthoritativeTail(true);
    const latestAgent = updated.chat.items.find((item) => item.message_id === "agent-later");
    if (latestAgent?.message.kind !== "agent_message" || latestAgent.message.parts[0]?.kind !== "text") {
      throw new Error("expected latest Agent text");
    }
    latestAgent.message.parts[0].text = "Latest update received live";
    act(() => {
      tree.update(
        <TaskView
          {...taskViewProps(updated)}
          liveTextPresentation={{
            agent: { messageId: "agent-later", eventCursor: "cursor-live-1" },
          }}
        />,
      );
    });

    const renderedTask = JSON.stringify(tree.toJSON());
    expect(renderedTask).toContain("Earlier response");
    expect(renderedTask).toContain("Steered follow-up");
    expect(renderedTask).toContain("npm test");
    expect(renderedTask).toContain("Latest update");
    expect(renderedTask).not.toContain("received live");
    expect(renderedTask).toContain("chat-streaming-caret");

    for (let frame = 0; frame < 40; frame += 1) {
      await act(async () => {
        await vi.advanceTimersByTimeAsync(20);
      });
    }
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });
    const settledTask = JSON.stringify(tree.toJSON());
    expect(settledTask).toContain("Latest update received live");
    expect(settledTask).not.toContain("chat-streaming-caret");
  });

  it("does not let a large received suffix build a visible presentation backlog", async () => {
    const { TaskView } = await import("./TaskView");
    const initial = snapshotWithAuthoritativeTail(true);
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<TaskView {...taskViewProps(initial)} />);
    });

    const updated = structuredClone(initial);
    const latestAgent = updated.chat.items.find((item) => item.message_id === "agent-later");
    if (latestAgent?.message.kind !== "agent_message" || latestAgent.message.parts[0]?.kind !== "text") {
      throw new Error("expected latest Agent text");
    }
    latestAgent.message.parts[0].text += `${" streamed".repeat(1_000)} END-OF-CHUNK`;
    act(() => {
      tree.update(
        <TaskView
          {...taskViewProps(updated)}
          liveTextPresentation={{
            agent: { messageId: "agent-later", eventCursor: "cursor-large-chunk" },
          }}
        />,
      );
    });

    expect(JSON.stringify(tree.toJSON())).not.toContain("END-OF-CHUNK");
    for (let frame = 0; frame < 6; frame += 1) {
      await act(async () => {
        await vi.advanceTimersByTimeAsync(20);
      });
    }
    expect(JSON.stringify(tree.toJSON())).toContain("END-OF-CHUNK");
  });

  it("continues smoothing chunks after the first presentation deadline", async () => {
    const { TaskView } = await import("./TaskView");
    const initial = snapshotWithAuthoritativeTail(true);
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<TaskView {...taskViewProps(initial)} />);
    });

    const firstUpdate = structuredClone(initial);
    const latestAgent = firstUpdate.chat.items.find((item) => item.message_id === "agent-later");
    if (latestAgent?.message.kind !== "agent_message" || latestAgent.message.parts[0]?.kind !== "text") {
      throw new Error("expected latest Agent text");
    }
    latestAgent.message.parts[0].text = "Latest update with the first streamed chunk";
    act(() => {
      tree.update(
        <TaskView
          {...taskViewProps(firstUpdate)}
          liveTextPresentation={{
            agent: { messageId: "agent-later", eventCursor: "cursor-continuous-1" },
          }}
        />,
      );
    });
    for (let frame = 0; frame < 6; frame += 1) {
      await act(async () => {
        await vi.advanceTimersByTimeAsync(20);
      });
    }
    expect(JSON.stringify(tree.toJSON())).toContain("Latest update with the first streamed chunk");

    const secondUpdate = structuredClone(firstUpdate);
    const continuingAgent = secondUpdate.chat.items.find((item) => item.message_id === "agent-later");
    if (continuingAgent?.message.kind !== "agent_message" || continuingAgent.message.parts[0]?.kind !== "text") {
      throw new Error("expected continuing Agent text");
    }
    continuingAgent.message.parts[0].text += " with a later substantial chunk";
    act(() => {
      tree.update(
        <TaskView
          {...taskViewProps(secondUpdate)}
          liveTextPresentation={{
            agent: { messageId: "agent-later", eventCursor: "cursor-continuous-2" },
          }}
        />,
      );
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(20);
    });

    expect(JSON.stringify(tree.toJSON())).not.toContain("with a later substantial chunk");
    for (let frame = 0; frame < 5; frame += 1) {
      await act(async () => {
        await vi.advanceTimersByTimeAsync(20);
      });
    }
    expect(JSON.stringify(tree.toJSON())).toContain("with a later substantial chunk");
  });

  it("shows authoritative text immediately while the document is hidden", async () => {
    vi.stubGlobal("document", {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      visibilityState: "hidden",
    });
    const { TaskView } = await import("./TaskView");
    const initial = snapshotWithAuthoritativeTail(true);
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<TaskView {...taskViewProps(initial)} />);
    });

    const updated = structuredClone(initial);
    const latestAgent = updated.chat.items.find((item) => item.message_id === "agent-later");
    if (latestAgent?.message.kind !== "agent_message" || latestAgent.message.parts[0]?.kind !== "text") {
      throw new Error("expected latest Agent text");
    }
    latestAgent.message.parts[0].text = "Latest update while hidden";
    act(() => {
      tree.update(
        <TaskView
          {...taskViewProps(updated)}
          liveTextPresentation={{
            agent: { messageId: "agent-later", eventCursor: "cursor-hidden" },
          }}
        />,
      );
    });

    const rendered = JSON.stringify(tree.toJSON());
    expect(rendered).toContain("Latest update while hidden");
    expect(rendered).not.toContain("chat-streaming-caret");
  });

  it("waits for the matching Chat update when its live signal is reduced first", async () => {
    const { TaskView } = await import("./TaskView");
    const initial = snapshotWithAuthoritativeTail(false);
    const signal = {
      agent: { messageId: "agent-later", eventCursor: "cursor-live-before-snapshot" },
    };
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<TaskView {...taskViewProps(initial)} />);
    });
    act(() => {
      tree.update(<TaskView {...taskViewProps(initial)} liveTextPresentation={signal} />);
    });

    const updated = snapshotWithAuthoritativeTail(true);
    const latestAgent = updated.chat.items.find((item) => item.message_id === "agent-later");
    if (latestAgent?.message.kind !== "agent_message" || latestAgent.message.parts[0]?.kind !== "text") {
      throw new Error("expected latest Agent text");
    }
    latestAgent.message.parts[0].text = "Latest update received after signal";
    act(() => {
      tree.update(<TaskView {...taskViewProps(updated)} liveTextPresentation={signal} />);
    });

    const renderedTask = JSON.stringify(tree.toJSON());
    expect(renderedTask).not.toContain("received after signal");
    expect(renderedTask).toContain("chat-streaming-caret");
  });

  it("animates at most the latest message in each Agent and Thought channel", async () => {
    const { TaskView } = await import("./TaskView");
    const initial = snapshotWithAuthoritativeTail(false);
    initial.chat.items = [
      thoughtText("thought-old", "Old thought"),
      agentText("agent-latest", "Agent answer"),
      thoughtText("thought-latest", "Latest thought"),
    ];
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<TaskView {...taskViewProps(initial)} />);
    });

    const lateOldThought = structuredClone(initial);
    const oldThought = lateOldThought.chat.items[0];
    if (oldThought?.message.kind !== "agent_message" || oldThought.message.parts[0]?.kind !== "text") {
      throw new Error("expected old Thought");
    }
    oldThought.message.parts[0].text = "Old thought arrived late";
    act(() => {
      tree.update(
        <TaskView
          {...taskViewProps(lateOldThought)}
          liveTextPresentation={{
            thought: { messageId: "thought-old", eventCursor: "cursor-thought-old" },
          }}
        />,
      );
    });
    expect(JSON.stringify(tree.toJSON())).toContain("Old thought arrived late");
    expect(JSON.stringify(tree.toJSON())).not.toContain("chat-streaming-caret");

    const liveLatest = structuredClone(lateOldThought);
    const agent = liveLatest.chat.items[1];
    const thought = liveLatest.chat.items[2];
    if (agent?.message.kind !== "agent_message" || agent.message.parts[0]?.kind !== "text"
      || thought?.message.kind !== "agent_message" || thought.message.parts[0]?.kind !== "text") {
      throw new Error("expected latest Agent and Thought text");
    }
    agent.message.parts[0].text = "Agent answer live";
    thought.message.parts[0].text = "Latest thought live";
    act(() => {
      tree.update(
        <TaskView
          {...taskViewProps(liveLatest)}
          liveTextPresentation={{
            agent: { messageId: "agent-latest", eventCursor: "cursor-agent-live" },
            thought: { messageId: "thought-latest", eventCursor: "cursor-thought-live" },
          }}
        />,
      );
    });

    expect(JSON.stringify(tree.toJSON()).match(/chat-streaming-caret/g)).toHaveLength(2);
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

  it("locks Configuration Options until the correlated change settles", async () => {
    const { TaskView } = await import("./TaskView");
    const snapshot = snapshotWithAuthoritativeTail(true);
    snapshot.agent_config = {
      agent_id: "codex",
      status: "ready",
      pending_change: {
        mutation_id: "mutation-1",
        option_id: "fast-mode",
        requested_value: "on",
      },
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
      tree = create(<TaskView {...taskViewProps(snapshot)} backendReady />);
    });

    const configControl = tree.root.find((node) =>
      typeof node.props.className === "string"
      && node.props.className.split(/\s+/).includes("composer-config-control"));
    expect(configControl.props.disabled).toBe(true);
  });

  it("renders an active request after the newest durable Chat row", async () => {
    const { TaskView } = await import("./TaskView");
    const current = snapshotWithAuthoritativeTail(true);
    current.active_requests = [{
      cursor: "pending-request-1",
      identity: "pending-request-1",
      message_id: "pending-request-1",
      message_type: "permission",
      message: {
        kind: "permission",
        id: "pending-request-1",
        request_id: "request-1",
        app_server_request_id: "request-1",
        title: "Approve final command",
        tool_call: { id: "tool-pending", title: "Final command", kind: "execute" },
        state: "pending",
        created_at: "2026-07-13T00:00:04Z",
        options: [{ id: "allow", label: "Allow", kind: "allow" }],
      },
    }];

    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<TaskView {...taskViewProps(current)} />);
    });

    const rendered = JSON.stringify(tree.toJSON());
    expect(rendered.indexOf("Final command")).toBeGreaterThan(rendered.indexOf("Latest update"));
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

    expect(JSON.stringify(tree.toJSON())).not.toContain("Reconnecting to App Server.");
    const editor = tree.root.findByProps({ role: "textbox", "aria-label": "Message" });
    expect(editor.props.contentEditable).toBe(true);
    expect(editor.props["aria-placeholder"]).toBe("Reconnecting. Draft is saved here.");
    expect(tree.root.findByProps({ "aria-label": "Send message" }).props.disabled).toBe(true);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });

    const rendered = JSON.stringify(tree.toJSON());
    expect(rendered).toContain("Reconnecting to App Server.");
    expect(rendered).toContain("App Server is temporarily unavailable.");
    expect(rendered).not.toContain("Connection closed.");
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
    backendReady: true,
    chatPageState: undefined,
    intents: {
      changePrompt: vi.fn(),
      recordScroll: vi.fn(),
      reportAttachmentError: vi.fn(),
    },
    onCancel: vi.fn(),
    onLoadChatPage: vi.fn(),
    onSubscribeToolDetail: vi.fn(() => vi.fn()),
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
  const items: ChatMessage[] = [agentText("agent-streaming", "Earlier response")];
  if (includeTail) {
    items.push(
      agentText("agent-later", "Latest update"),
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
    );
  }

  return {
    lifecycle: "visible",
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
    active_requests: [],
    send_capability: { state: "ready" },
    settings_summary: { agent_id: "codex", isolation: "local" },
    revision,
  };
}

function agentText(messageId: string, text: string): ChatMessage {
  return {
    cursor: messageId,
    identity: messageId,
    message_id: messageId,
    message_type: "agent_message",
    message: {
      kind: "agent_message",
      id: messageId,
      role: "agent",
      parts: [{ kind: "text", text }],
      created_at: "2026-07-12T00:00:00Z",
    },
  };
}

function thoughtText(messageId: string, text: string): ChatMessage {
  return {
    cursor: messageId,
    identity: messageId,
    message_id: messageId,
    message_type: "agent_message",
    message: {
      kind: "agent_message",
      id: messageId,
      role: "thought",
      parts: [{ kind: "text", text }],
      created_at: "2026-07-12T00:00:00Z",
    },
  };
}
