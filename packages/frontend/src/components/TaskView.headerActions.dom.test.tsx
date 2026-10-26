// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import type { ChatMessage, TaskSnapshot } from "@openaide/app-shell-contracts";
import { TaskView } from "./TaskView";
import { TerminalSurfaceContext } from "./terminals/terminalSurface";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;

afterEach(() => {
  act(() => root?.unmount());
  root = undefined;
  document.body.replaceChildren();
});

it("keeps one message picker in the mobile header across task switches", () => {
  const header = document.createElement("div");
  const host = document.createElement("div");
  document.body.append(header, host);
  root = createRoot(host);

  for (const taskId of ["task-a", "task-b", "task-a"]) {
    act(() => root!.render(<TaskView {...taskViewProps(snapshot(taskId))} headerActionsTarget={header} />));
    expect(header.querySelectorAll('[aria-label="Jump to a message"]')).toHaveLength(1);
  }

  act(() => root!.unmount());
  root = undefined;
  expect(header.childElementCount).toBe(0);
});

it("opens the Task's Project Terminals from the header, starting new ones in its Workspace", () => {
  // jsdom has no layout; the message picker scrolls its marker into view on mount.
  Element.prototype.scrollIntoView ??= vi.fn();
  const host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  const openPage = vi.fn();
  const task = snapshot("task-a");
  task.task.project_id = "project-1";
  const terminals = [{ terminalId: "t1", projectId: "project-1" as never, folder: "/w", status: "busy" as const, openedAt: 1 }];

  act(() => root!.render(
    <TerminalSurfaceContext.Provider value={{ terminals, openPage }}>
      <TaskView {...taskViewProps(task)} />
    </TerminalSurfaceContext.Provider>,
  ));
  const entry = host.querySelector<HTMLButtonElement>('[aria-label="Terminal, 1 open"]');
  act(() => entry!.click());

  expect(openPage).toHaveBeenCalledWith({ projectId: "project-1", taskId: "task-a" });
});

it("moves the rarer Task controls from the phone bar to More", () => {
  Element.prototype.scrollIntoView ??= vi.fn();
  const bar = document.createElement("div");
  const more = document.createElement("div");
  const host = document.createElement("div");
  document.body.append(bar, more, host);
  root = createRoot(host);
  const task = snapshot("task-a");
  task.task.project_id = "project-1";

  act(() => root!.render(
    <TerminalSurfaceContext.Provider value={{ terminals: [], openPage: vi.fn() }}>
      <TaskView {...taskViewProps(task)} headerActionsTarget={bar} headerOverflowTarget={more} />
    </TerminalSurfaceContext.Provider>,
  ));

  expect(bar.childElementCount).toBe(0);
  expect(more.querySelector('[aria-label="Terminal"]')).not.toBeNull();
  expect(more.querySelector('[aria-label="Jump to a message"]')).not.toBeNull();
});

function taskViewProps(taskSnapshot: TaskSnapshot) {
  return {
    backendReady: true,
    chatPageState: undefined,
    intents: {
      changePrompt: vi.fn(),
      recordScroll: vi.fn(),
      refreshWorkspace: vi.fn(),
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
    snapshot: taskSnapshot,
    taskInput: { prompt: "", context: [] },
    toolDetails: {},
    submitShortcut: "mod_enter" as const,
  };
}

function snapshot(taskId: string): TaskSnapshot {
  const items = [userText(`${taskId}-first`, "First question"), userText(`${taskId}-second`, "Second question")];
  return {
    lifecycle: "open",
    permission_policy: "ask_every_time",
    task: {
      task_id: taskId,
      title: "Task",
      status: "inactive",
      task_version: 1,
      message_history_version: 1,
      has_messages: true,
      unread: false,
      pinned: false,
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
      task_id: taskId,
      items,
      has_before: false,
      has_messages: true,
      total_count: items.length,
      version: 1,
    },
    active_requests: [],
    send_capability: { state: "ready" },
    settings_summary: { agent_id: "codex", isolation: "local" },
    revision: 1,
  };
}

function userText(messageId: string, text: string): ChatMessage {
  return {
    cursor: messageId,
    identity: messageId,
    message_id: messageId,
    message_type: "user",
    message: { kind: "user", id: messageId, text, created_at: "2026-07-12T00:00:00Z" },
  };
}
