import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import type {
  AppServerSession,
  AppServerStateObserver,
  SubscriptionScope,
  ReplicaSnapshot,
} from "@openaide/app-server-client";

const SUBAGENT_ID = "subagent_11111111111111111111111111111111";

describe("TaskView Composer across Subagent inspection", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    vi.useFakeTimers();
    vi.stubGlobal("window", {
      acquireVsCodeApi: undefined,
      addEventListener: vi.fn(),
      clearTimeout: globalThis.clearTimeout,
      removeEventListener: vi.fn(),
      setTimeout: globalThis.setTimeout,
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("does not replay an already inserted Quote when returning to Main Agent", async () => {
    const { TaskView } = await import("./TaskView");
    const observers: Array<{ scope: SubscriptionScope; observer: AppServerStateObserver }> = [];
    const subagentConnection = {
      request: vi.fn(),
      subscribeState(scope: SubscriptionScope, observer: AppServerStateObserver) {
        observers.push({ scope, observer });
        return () => undefined;
      },
    } as unknown as Pick<AppServerSession, "request" | "subscribeState">;
    const changePrompt = vi.fn();
    let tree!: ReactTestRenderer;

    await act(async () => {
      tree = create(
        <TaskView
          backendReady
          chatPageState={undefined}
          intents={{
            changePrompt,
            recordScroll: vi.fn(),
            refreshWorkspace: vi.fn(),
            reportAttachmentError: vi.fn(),
          } as never}
          onCancel={vi.fn()}
          onLoadChatPage={vi.fn()}
          onPermissionRespond={vi.fn()}
          onRemoveAttachment={vi.fn()}
          onRevealAttachment={vi.fn()}
          onSelectConfigOption={vi.fn()}
          onSendPrompt={vi.fn()}
          onSubscribeToolDetail={vi.fn(() => vi.fn())}
          permissionResponses={{}}
          snapshot={taskSnapshot()}
          subagentConnection={subagentConnection}
          submitShortcut="mod_enter"
          taskInput={{ prompt: "", context: [] }}
          toolDetails={{}}
        />,
      );
    });
    const catalog = observers.find(({ scope }) => scope.kind === "subagentCatalog")!;
    await act(async () => catalog.observer.onSnapshot(catalogSnapshot()));

    const timeline = () => tree.root.find((node) =>
      typeof node.props.onOpenSubagent === "function" && "liveTextPresentation" in node.props);
    await act(async () => timeline().props.onQuote("Quoted earlier"));
    expect(changePrompt).toHaveBeenCalledTimes(1);
    expect(changePrompt).toHaveBeenLastCalledWith("> Quoted earlier\n");

    await act(async () => timeline().props.onOpenSubagent(SUBAGENT_ID));
    expect(tree.root.findAllByProps({ role: "textbox", "aria-label": "Message" })).toHaveLength(0);
    await act(async () => {
      tree.root.findByProps({ className: "subagent-inspection-return" }).props.onClick();
    });

    expect(tree.root.findAllByProps({ role: "textbox", "aria-label": "Message" })).toHaveLength(1);
    expect(changePrompt).toHaveBeenCalledTimes(1);

    // Quote in Main Agent Chat from a Subagent history still reaches the remounted Composer once.
    await act(async () => timeline().props.onOpenSubagent(SUBAGENT_ID));
    await act(async () => timeline().props.onQuote("From subagent"));
    expect(tree.root.findAllByProps({ role: "textbox", "aria-label": "Message" })).toHaveLength(1);
    expect(changePrompt).toHaveBeenCalledTimes(2);
    expect(changePrompt).toHaveBeenLastCalledWith("> From subagent\n");
  });
});

function catalogSnapshot(): ReplicaSnapshot {
  return {
    kind: "subagentCatalog",
    catalog: {
      taskId: "task-1",
      revision: 1,
      entries: [{
        subagentId: SUBAGENT_ID,
        name: "Explorer",
        delegatedTask: "Inspect the code",
        status: "running",
        capabilities: { cancel: false, close: false },
        spawnedOrder: 1,
        historyRevision: 0,
      }],
    },
  } as unknown as ReplicaSnapshot;
}

function taskSnapshot(): TaskSnapshot {
  return {
    lifecycle: "open",
    permission_policy: "ask_every_time",
    task: {
      task_id: "task-1",
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
    chat: { task_id: "task-1", items: [], has_before: false, has_messages: true, total_count: 0, version: 1 },
    active_requests: [],
    send_capability: { state: "ready" },
    settings_summary: { agent_id: "codex", isolation: "local" },
    revision: 1,
  };
}
