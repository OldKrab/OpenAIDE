// @vitest-environment jsdom

import type { ReactElement } from "react";
import { act, create, type ReactTestRendererNode } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatItem, MessageId, PendingRequestSnapshot, RequestId, TaskId } from "@openaide/app-server-client";
import type { TaskSummary } from "@openaide/app-shell-contracts";

vi.mock("./AgentMarkdown", () => ({
  AgentMarkdown: ({ text }: { text: string }) => <div>{text}</div>,
}));

import { mapProtocolChatItem, pendingRequestItems } from "../state/appServerProtocolChatMapping";
import { ChatRow } from "./ChatMessageView";
import { SidebarTaskRow } from "./SidebarTaskRow";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const NOW = Date.parse("2026-01-01T10:00:00Z");
const ago = (seconds: number) => String(NOW - seconds * 1_000);

describe("time information from App Server-observed timing", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("shows how long the turn worked on the answer that closed it", () => {
    const tree = row(item({
      parts: [{ kind: "text", text: "Done." }],
      timing: { closedTurn: { startedAt: ago(252), endedAt: ago(0) } },
    }));

    expect(text(tree)).toContain("Worked for 4m 12s");
    expect(times(tree)).toEqual([{ children: "4m 12s", dateTime: "PT252S" }]);
  });

  it("shows no turn time on an answer the App Server did not watch finish", () => {
    const tree = row(item({ parts: [{ kind: "text", text: "Loaded from history." }] }));

    expect(text(tree)).not.toContain("Worked for");
    expect(times(tree)).toEqual([]);
  });

  it("reports how long a stopped turn ran on its interruption line", () => {
    const tree = row(item({
      status: "interrupted",
      parts: [{ kind: "text", text: "Task was interrupted." }],
      timing: { closedTurn: { startedAt: ago(80), endedAt: ago(0) } },
    }));

    expect(text(tree)).toContain("Task was interrupted. · 1m 20s");
  });

  it("shows when a user message was sent as a clock time, not a duration", () => {
    const tree = row(item({
      role: "user",
      parts: [{ kind: "text", text: "Please fix it" }],
      timing: { sentAt: ago(600) },
    }));

    expect(tree.root.findByType("time").props.dateTime).toBe("2026-01-01T09:50:00.000Z");
  });

  it("adds the duration to a finished compaction and none to one it did not watch", () => {
    const compaction = (timing?: ChatItem["timing"]) => row(item({
      parts: [{ kind: "compaction", status: "completed" }],
      timing,
    }));

    expect(times(compaction({ run: { startedAt: ago(42), endedAt: ago(0) } })))
      .toEqual([{ children: "42s", dateTime: "PT42S" }]);
    expect(times(compaction())).toEqual([]);
  });

  it("times a finished step and keeps a running step's clock ticking", () => {
    const step = (status: "completed" | "running", run: { startedAt: string; endedAt?: string }) => row(item({
      parts: [{
        kind: "activity",
        title: "Read",
        status,
        steps: [{ kind: "tool", name: "read", status, toolCallId: "call_1", inputSummary: "notes.md", permissionOutcomes: [] }],
      }],
      timing: { run },
    }));

    const finished = step("completed", { startedAt: ago(41), endedAt: ago(0) });
    expect(times(finished)).toContainEqual({ children: "41s", dateTime: "PT41S" });

    const running = step("running", { startedAt: ago(35) });
    expect(liveTimes(running)).toContain("0:35");
    act(() => {
      vi.advanceTimersByTime(2_000);
    });
    expect(liveTimes(running)).toContain("0:37");
  });

  it("shows how long a pending permission has waited", () => {
    const request: PendingRequestSnapshot = {
      requestId: "request_1" as RequestId,
      scope: { kind: "task", taskId: "task_1" as TaskId },
      kind: "permission",
      title: "Run command",
      permission: {
        title: "Run command",
        toolCall: { id: "call_1", title: "npm test" },
        options: [{ optionId: "allow", name: "Allow", kind: "allowOnce" }],
      },
      createdAt: ago(130),
    };
    const [message] = pendingRequestItems([request], ago(0));
    const tree = render(<ChatRow message={message} onPermissionRespond={vi.fn()} taskId="task_1" />);

    expect(text(tree)).toContain("waiting 2:10");
  });

  it("shows running and waiting time in Navigation, and nothing for a settled Task", () => {
    const running = sidebarRow({ status: "active", active_turn_started_at: ago(75) });
    const waiting = sidebarRow({
      status: "waiting",
      attention: { event_id: "event_1", reason: "needsPermission", occurred_at: ago(130) },
    });
    const settled = sidebarRow({ status: "inactive", active_turn_started_at: undefined });

    expect(liveTimes(running)).toEqual(["1:15"]);
    expect(liveTimes(waiting)).toEqual(["2:10"]);
    expect(liveTimes(settled)).toEqual([]);
  });
});

function item(overrides: Partial<ChatItem>): ChatItem {
  return { messageId: "message_1" as MessageId, role: "agent", status: "complete", parts: [], ...overrides };
}

function row(chatItem: ChatItem) {
  const message = mapProtocolChatItem(chatItem, ago(0));
  return render(<ChatRow message={message} onPermissionRespond={vi.fn()} taskId="task_1" />);
}

function sidebarRow(overrides: Partial<TaskSummary>) {
  const task: TaskSummary = {
    agent_id: "codex",
    agent_name: "Codex",
    created_at: ago(0),
    isolation: "local",
    last_activity: ago(0),
    message_history_version: 1,
    has_messages: true,
    status: "inactive",
    task_id: "task",
    task_version: 1,
    title: "Task",
    unread: false,
    pinned: false,
    updated_at: ago(0),
    workspace_root: "",
    ...overrides,
  };
  return render(
    <SidebarTaskRow
      onArchiveTask={vi.fn()}
      onOpenTask={vi.fn()}
      onRestoreTask={vi.fn()}
      showArchived={false}
      task={task}
    />,
  );
}

function render(element: ReactElement) {
  let tree!: ReturnType<typeof create>;
  act(() => {
    tree = create(element);
  });
  return tree;
}

function times(tree: ReturnType<typeof create>) {
  return tree.root.findAllByType("time").map((time) => ({
    children: time.props.children,
    dateTime: time.props.dateTime,
  }));
}

function liveTimes(tree: ReturnType<typeof create>) {
  return tree.root.findAllByType("time").map((time) => time.props.children as string);
}

function text(tree: ReturnType<typeof create>) {
  const collect = (node: ReactTestRendererNode): string =>
    typeof node === "string" ? node : (node.children ?? []).map(collect).join("");
  const json = tree.toJSON();
  if (json === null) return "";
  return (Array.isArray(json) ? json : [json]).map(collect).join("");
}
