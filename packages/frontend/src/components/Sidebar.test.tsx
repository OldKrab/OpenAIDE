import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentListedSession, TaskSummary } from "@openaide/app-shell-contracts";
import type { NativeSessionsState } from "../state/store";
import { Sidebar } from "./Sidebar";
import { SidebarNativeSessionRow } from "./SidebarNativeSessionRow";
import { SidebarTaskRow } from "./SidebarTaskRow";
import { SidebarTaskPreviewProvider } from "./SidebarTaskPreview";
import { sidebarViewModel } from "./sidebarViewModel";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

describe("sidebarViewModel", () => {
  it("filters native sessions by title and agent metadata outside archive mode", () => {
    const model = sidebarViewModel({
      nativeSessionAgentName: "Codex",
      nativeSessions: nativeSessions({
        items: [
          nativeSession({ session_id: "session_1", title: "Refactor plan" }),
          nativeSession({ session_id: "session_2", title: "Release notes" }),
        ],
      }),
      searchQuery: "  codex  ",
      showArchived: false,
      taskCount: 1,
    });

    expect(model.visibleNativeSessions.map((session) => session.session_id)).toEqual(["session_1", "session_2"]);
    expect(model.visibleCount).toBe(3);
  });

  it("uses archive empty state and excludes native sessions from archive count", () => {
    const model = sidebarViewModel({
      nativeSessionAgentName: "Codex",
      nativeSessions: nativeSessions({ items: [nativeSession({ session_id: "session_1" })] }),
      searchQuery: "",
      showArchived: true,
      taskCount: 0,
    });

    expect(model.visibleCount).toBe(0);
    expect(model.emptyMessage).toBe("Archive is empty. Archived tasks will appear here.");
  });

  it("keeps loading and search empty states distinct", () => {
    const loading = sidebarViewModel({
      nativeSessionAgentName: "Codex",
      nativeSessions: nativeSessions({ loading: true }),
      searchQuery: "",
      showArchived: false,
      taskCount: 0,
    });
    const searching = sidebarViewModel({
      nativeSessionAgentName: "Codex",
      nativeSessions: nativeSessions(),
      searchQuery: "missing",
      showArchived: false,
      taskCount: 0,
    });

    expect(loading.emptyMessage).toBe("Loading tasks.");
    expect(searching.emptyMessage).toBe("No matching tasks.");
  });
});

describe("SidebarTaskRow", () => {
  it("uses the agent icon as the stable leading marker", () => {
    const tree = render(
      <SidebarTaskRow
        onArchiveTask={vi.fn()}
        onOpenTask={vi.fn()}
        onRestoreTask={vi.fn()}
        showArchived={false}
        task={task({ agent_id: "opencode", agent_name: "OpenCode" })}
      />,
    );

    expect(tree.root.findByProps({ className: "task-agent-icon" }).props["aria-label"]).toBe("Agent: OpenCode");
  });

  it("uses a worktree marker without adding a second task line", () => {
    const tree = render(
      <SidebarTaskRow
        onArchiveTask={vi.fn()}
        onOpenTask={vi.fn()}
        onRestoreTask={vi.fn()}
        showArchived={false}
        task={task({
          worktree_id: "worktree_1",
          worktree_name: "Sidebar scrolling",
          git_ref: "fix/sidebar-scroll",
        })}
      />,
    );

    expect(tree.root.findByProps({ className: "task-agent-icon" }).props["aria-label"])
      .toBe("Agent: Codex");
    expect(tree.root.findByProps({ className: "task-worktree-marker" }).props["aria-label"])
      .toBe("Worktree: Sidebar scrolling");
    expect(tree.root.findAllByProps({ className: "task-title" })).toHaveLength(1);
    expect(tree.root.findAllByProps({ className: "task-subtitle" })).toHaveLength(0);
  });

  it("renders live, waiting, failed, unread, and age states in one trailing slot", () => {
    const renderState = (status: TaskSummary["status"], unread = false) => render(
      <SidebarTaskRow
        onArchiveTask={vi.fn()}
        onOpenTask={vi.fn()}
        onRestoreTask={vi.fn()}
        showArchived={false}
        task={task({ status, task_id: `task_${status}_${unread}`, unread })}
      />,
    );

    const active = renderState("active", true);
    expect(active.root.findByProps({ "aria-label": "In progress" })).toBeDefined();
    expect(active.root.findAllByProps({ className: "task-state-unread-badge" })).toHaveLength(0);
    expect(active.root.findAllByProps({ className: "task-meta-age" })).toHaveLength(0);

    const waiting = renderState("waiting");
    expect(waiting.root.findByProps({ "aria-label": "Waiting" })).toBeDefined();
    const waitingUnread = renderState("waiting", true);
    expect(waitingUnread.root.findByProps({ "aria-label": "Waiting, unread" })).toBeDefined();
    expect(waitingUnread.root.findAllByProps({ className: "task-state-unread-badge" })).toHaveLength(1);

    const failedUnread = renderState("failed", true);
    expect(failedUnread.root.findByProps({ "aria-label": "Failed, unread" })).toBeDefined();
    expect(failedUnread.root.findAllByProps({ className: "task-state-error" })).toHaveLength(1);

    const unread = renderState("inactive", true);
    expect(unread.root.findByProps({ "aria-label": "Unread" })).toBeDefined();
    expect(unread.root.findAllByProps({ className: "task-state-unread-dot" })).toHaveLength(1);

    const read = renderState("completed");
    expect(read.root.findAllByProps({ className: "task-meta-age" })).toHaveLength(1);
  });

  it("opens selected tasks and exposes Archive beside the Task details action", () => {
    const onOpenTask = vi.fn();
    const onArchiveTask = vi.fn();
    const tree = render(
      <SidebarTaskRow
        activeTaskId="task_1"
        onArchiveTask={onArchiveTask}
        onOpenTask={onOpenTask}
        onRestoreTask={vi.fn()}
        showArchived={false}
        task={task({ task_id: "task_1", unread: true })}
      />,
    );

    const buttons = tree.root.findAllByType("button");
    expect(tree.root.findByProps({ role: "listitem" }).props.className).toBe("task-row task-product-row selected");
    expect(tree.root.findByProps({ "aria-label": "Unread" })).toBeDefined();

    act(() => buttons[0].props.onClick());
    act(() => tree.root.findByProps({ "aria-label": "Task actions for Task" }).props.onClick());
    const menuItems = tree.root.findAllByProps({ role: "menuitem" });
    expect(menuItems).toHaveLength(2);
    expect(menuItems[0].props.className).toBe("task-row-details-action");
    expect(menuItems[1].children).toContain("Archive task");
    act(() => menuItems[1].props.onClick());

    expect(onOpenTask).toHaveBeenCalledWith("task_1");
    expect(onArchiveTask).toHaveBeenCalledWith("task_1");
  });

  it("shows the complete desktop preview content through the Task details action", () => {
    const tree = render(
      <SidebarTaskRow
        onArchiveTask={vi.fn()}
        onOpenTask={vi.fn()}
        onRestoreTask={vi.fn()}
        showArchived={false}
        task={task({
          status: "failed",
          title: "Popup work",
          project_label: "OpenAIDE",
          worktree_id: "worktree_1",
          worktree_name: "Sidebar scrolling",
          git_ref: "fix/sidebar-scroll",
        })}
      />,
    );

    act(() => tree.root.findByProps({ "aria-label": "Task actions for Popup work" }).props.onClick());
    act(() => tree.root.findByProps({ className: "task-row-details-action" }).props.onClick());

    const details = tree.root.findByProps({ className: "task-row-details" });
    const text = details.findAllByType("strong").map((item) => item.children.join(""));
    expect(text).toEqual(["Popup work", "OpenAIDE", "Sidebar scrolling"]);
    expect(details.findByProps({ className: "task-preview-state" }).children).toContain("Failed");
    expect(details.findByType("em").children).toContain("fix/sidebar-scroll");
  });

  it("renames and resets a user-owned Task title from the row menu", async () => {
    const onSetTaskTitle = vi.fn().mockResolvedValue(undefined);
    const tree = render(
      <SidebarTaskRow
        onArchiveTask={vi.fn()}
        onOpenTask={vi.fn()}
        onRestoreTask={vi.fn()}
        onSetTaskTitle={onSetTaskTitle}
        showArchived={false}
        task={task({ task_id: "task_rename", title: "Agent title", title_source: "user" })}
      />,
    );

    act(() => tree.root.findByProps({ "aria-label": "Task actions for Agent title" }).props.onClick());
    act(() => tree.root.findAllByProps({ role: "menuitem" })
      .find((item) => item.children.includes("Rename task"))!.props.onClick());
    const input = tree.root.findByProps({ "aria-label": "Rename Agent title" });
    act(() => input.props.onChange({ target: { value: "My title" } }));
    await act(async () => {
      tree.root.findByProps({ className: "task-rename-form" }).props.onSubmit({
        preventDefault: vi.fn(),
      });
    });

    expect(onSetTaskTitle).toHaveBeenCalledWith("task_rename", {
      kind: "user",
      value: "My title",
    });

    act(() => tree.root.findByProps({ "aria-label": "Task actions for Agent title" }).props.onClick());
    await act(async () => {
      tree.root.findAllByProps({ role: "menuitem" })
        .find((item) => item.children.includes("Reset to Agent title"))!.props.onClick();
    });
    expect(onSetTaskTitle).toHaveBeenLastCalledWith("task_rename", { kind: "automatic" });
  });

  it("dismisses the task actions menu on outside click and Escape", () => {
    const listeners = new Map<string, EventListener>();
    const fakeDocument = {
      addEventListener: vi.fn((type: string, listener: EventListener) => listeners.set(type, listener)),
      removeEventListener: vi.fn((type: string, listener: EventListener) => {
        if (listeners.get(type) === listener) listeners.delete(type);
      }),
    };
    vi.stubGlobal("document", fakeDocument);
    try {
      const tree = render(
        <SidebarTaskRow
          onArchiveTask={vi.fn()}
          onOpenTask={vi.fn()}
          onRestoreTask={vi.fn()}
          showArchived={false}
          task={task({ task_id: "task_dismiss", title: "Dismiss menu" })}
        />,
      );
      const trigger = tree.root.findByProps({ "aria-label": "Task actions for Dismiss menu" });

      act(() => trigger.props.onClick());
      expect(tree.root.findAllByProps({ role: "menu" })).toHaveLength(1);
      act(() => listeners.get("pointerdown")?.({ target: {} } as unknown as Event));
      expect(tree.root.findAllByProps({ role: "menu" })).toHaveLength(0);

      act(() => trigger.props.onClick());
      expect(tree.root.findAllByProps({ role: "menu" })).toHaveLength(1);
      act(() => listeners.get("keydown")?.({ key: "Escape", preventDefault: vi.fn() } as unknown as Event));
      expect(tree.root.findAllByProps({ role: "menu" })).toHaveLength(0);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("keeps the task preview dismissed while the actions menu is open", () => {
    vi.useFakeTimers();
    vi.stubGlobal("window", {
      innerHeight: 800,
      innerWidth: 1200,
      matchMedia: () => ({ matches: false }),
    });
    vi.stubGlobal("document", {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });
    const rowNode = {
      getBoundingClientRect: () => ({
        bottom: 72,
        height: 32,
        left: 8,
        right: 296,
        top: 40,
        width: 288,
        x: 8,
        y: 40,
      }),
    } as HTMLElement;
    try {
      const tree = render(
        <SidebarTaskPreviewProvider>
          <SidebarTaskRow
            onArchiveTask={vi.fn()}
            onOpenTask={vi.fn()}
            onRestoreTask={vi.fn()}
            showArchived={false}
            task={task({ task_id: "task_popup", title: "Popup task" })}
          />
        </SidebarTaskPreviewProvider>,
        {
          createNodeMock: (element) => (
            (element.props as { className?: string }).className?.startsWith("task-row task-product-row")
              ? rowNode
              : null
          ),
        },
      );
      const row = tree.root.findByProps({ role: "listitem" });

      act(() => row.props.onPointerMove());
      act(() => tree.root.findByProps({ "aria-label": "Task actions for Popup task" }).props.onClick());
      act(() => {
        vi.advanceTimersByTime(1_000);
      });

      expect(tree.root.findAllByProps({ role: "menu" })).toHaveLength(1);
      expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(0);
    } finally {
      vi.useRealTimers();
      vi.unstubAllGlobals();
    }
  });

  it("does not open the task preview when the actions button receives focus", () => {
    vi.stubGlobal("window", {
      matchMedia: () => ({ matches: false }),
    });
    vi.stubGlobal("document", {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });
    const rowNode = {} as HTMLElement;
    try {
      const tree = render(
        <SidebarTaskPreviewProvider>
          <SidebarTaskRow
            onArchiveTask={vi.fn()}
            onOpenTask={vi.fn()}
            onRestoreTask={vi.fn()}
            showArchived={false}
            task={task({ task_id: "task_focus", title: "Focus task" })}
          />
        </SidebarTaskPreviewProvider>,
        {
          createNodeMock: (element) => (
            (element.props as { className?: string }).className?.startsWith("task-row task-product-row")
              ? rowNode
              : null
          ),
        },
      );
      const actions = tree.root.findByProps({ "aria-label": "Task actions for Focus task" });

      act(() => actions.props.onFocus?.());

      expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(0);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("opens a pointer preview only after the pointer actually moves over the row", () => {
    vi.useFakeTimers();
    vi.stubGlobal("window", {
      matchMedia: () => ({ matches: false }),
    });
    vi.stubGlobal("document", {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });
    const rowNode = {} as HTMLElement;
    try {
      const tree = render(
        <SidebarTaskPreviewProvider>
          <SidebarTaskRow
            onArchiveTask={vi.fn()}
            onOpenTask={vi.fn()}
            onRestoreTask={vi.fn()}
            showArchived={false}
            task={task({ task_id: "task_move", title: "Move task" })}
          />
        </SidebarTaskPreviewProvider>,
        {
          createNodeMock: (element) => (
            (element.props as { className?: string }).className?.startsWith("task-row task-product-row")
              ? rowNode
              : null
          ),
        },
      );
      const row = tree.root.findByProps({ role: "listitem" });

      act(() => {
        vi.advanceTimersByTime(1_000);
      });
      expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(0);

      act(() => row.props.onPointerMove());
      act(() => {
        vi.advanceTimersByTime(1_000);
      });
      expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(1);
    } finally {
      vi.useRealTimers();
      vi.unstubAllGlobals();
    }
  });

  it("restores archived tasks through the archive action", () => {
    const onRestoreTask = vi.fn();
    const tree = render(
      <SidebarTaskRow
        onArchiveTask={vi.fn()}
        onOpenTask={vi.fn()}
        onRestoreTask={onRestoreTask}
        showArchived={true}
        task={task({ task_id: "task_2", title: "Archived task" })}
      />,
    );

    act(() => tree.root.findByProps({ "aria-label": "Task actions for Archived task" }).props.onClick());
    act(() => tree.root.findAllByProps({ role: "menuitem" })[1].props.onClick());

    expect(onRestoreTask).toHaveBeenCalledWith("task_2");
  });

  it("renders Agent titles without rewriting numeric suffixes", () => {
    const tree = render(
      <SidebarTaskRow
        onArchiveTask={vi.fn()}
        onOpenTask={vi.fn()}
        onRestoreTask={vi.fn()}
        showArchived={false}
        task={task({ title: "QA reload recovery 1782881988" })}
      />,
    );

    expect(tree.root.findByProps({ className: "task-title" }).children.join(""))
      .toBe("QA reload recovery 1782881988");
    expect(tree.root.findByProps({ className: "task-title" }).props.title).toBeUndefined();
    expect(tree.root.findByProps({ className: "task-meta-age" }).props.title).toBe(
      "Last activity: 2026-05-22T00:00:00.000Z",
    );
    expect(tree.root.findAllByProps({ className: "task-meta-reference" })).toHaveLength(0);
  });

  it("does not duplicate the rich preview with a native title tooltip", () => {
    const fullTitle = "A long task title that is trimmed by the sidebar width";
    const tree = render(
      <SidebarTaskRow
        onArchiveTask={vi.fn()}
        onOpenTask={vi.fn()}
        onRestoreTask={vi.fn()}
        showArchived={false}
        task={task({ title: fullTitle })}
      />,
    );

    expect(tree.root.findByProps({ className: "task-title" }).props.title).toBeUndefined();
  });

  it("renders the full Agent title when it contains machine-looking text", () => {
    const tree = render(
      <SidebarTaskRow
        onArchiveTask={vi.fn()}
        onOpenTask={vi.fn()}
        onRestoreTask={vi.fn()}
        showArchived={false}
        task={task({ title: "QA multitab long 1782881214972: retry run" })}
      />,
    );

    expect(tree.root.findByProps({ className: "task-title" }).children.join(""))
      .toBe("QA multitab long 1782881214972: retry run");
    expect(tree.root.findByProps({ className: "task-title" }).props.title).toBeUndefined();
    expect(tree.root.findAllByProps({ className: "task-meta-reference" })).toHaveLength(0);
  });
});

describe("SidebarNativeSessionRow", () => {
  it("uses the shared agent-left layout while opening task history", () => {
    const tree = render(
      <SidebarNativeSessionRow
        nativeSessionAgentId="codex"
        nativeSessionAgentName="Codex"
        nativeSessionsAdoptingSessionId="session_1"
        onOpenNativeSession={vi.fn()}
        session={nativeSession({ session_id: "session_1" })}
      />,
    );

    expect(tree.root.findByProps({ className: "task-agent-icon" }).props["aria-label"]).toBe("Agent: Codex");
    expect(tree.root.findByProps({ className: "task-trailing-indicator" }).props["aria-label"]).toBe("Opening task");
    expect(tree.root.findAllByProps({ className: "state-mark external-session-mark active" })).toHaveLength(0);
  });

  it("opens listed task history without session-facing copy", () => {
    const session = nativeSession({ session_id: "session_1", title: "Existing session" });
    const onOpenNativeSession = vi.fn();
    const tree = render(
      <SidebarNativeSessionRow
        nativeSessionAgentId="codex"
        nativeSessionAgentName="Codex"
        onOpenNativeSession={onOpenNativeSession}
        session={session}
      />,
    );

    const buttons = tree.root.findAllByType("button");
    expect(buttons[0].props.disabled).toBe(false);
    expect(buttons[1].props.title).toBe("Open task");
    expect(buttons[2].props.title).toBe("Task actions");
    expect(tree.root.findByProps({ className: "task-agent-icon" }).props["aria-label"]).toBe("Agent: Codex");
    expect(tree.root.findByProps({ className: "task-meta-age" })).toBeDefined();
    expect(tree.root.findByProps({ className: "task-title" }).props.title).toBeUndefined();

    act(() => buttons[0].props.onClick());

    expect(onOpenNativeSession).toHaveBeenCalledWith(session);
  });

  it("shows complete Task details for listed Agent history", () => {
    const tree = render(
      <SidebarNativeSessionRow
        nativeSessionAgentId="codex"
        nativeSessionAgentName="Codex"
        onOpenNativeSession={vi.fn()}
        session={nativeSession({
          cwd: "/workspace/OpenAIDE",
          last_activity: "2026-07-20T10:00:00Z",
          title: "Existing session",
        })}
      />,
    );

    act(() => tree.root.findByProps({ "aria-label": "Task actions for Existing session" }).props.onClick());
    act(() => tree.root.findByProps({ className: "task-row-details-action" }).props.onClick());

    const details = tree.root.findByProps({ className: "task-row-details" });
    expect(details.findAllByType("strong").map((item) => item.children.join("")))
      .toEqual(["Existing session", "/workspace/OpenAIDE"]);
    expect(details.findByProps({ className: "task-preview-source-action" }).children.join(""))
      .toBe("· Open to load");
  });

  it("shows the delayed rich preview for a Native Session that is not yet adopted", () => {
    vi.useFakeTimers();
    vi.stubGlobal("window", {
      innerHeight: 800,
      innerWidth: 1200,
      matchMedia: () => ({ matches: false }),
    });
    vi.stubGlobal("document", {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });
    const rowNode = {
      getBoundingClientRect: () => ({ bottom: 72, height: 32, left: 8, right: 296, top: 40, width: 288, x: 8, y: 40 }),
    } as HTMLElement;
    const onOpenNativeSession = vi.fn();
    const session = nativeSession({ cwd: "/workspace/OpenAIDE", title: "Existing session" });
    const tree = render(
      <SidebarTaskPreviewProvider>
        <SidebarNativeSessionRow
          nativeSessionAgentId="codex"
          nativeSessionAgentName="Codex"
          onOpenNativeSession={onOpenNativeSession}
          session={session}
        />
      </SidebarTaskPreviewProvider>,
      { createNodeMock: (element) => (element.props as { className?: string }).className === "task-row external-session-row" ? rowNode : null },
    );
    const row = tree.root.findByProps({ className: "task-row external-session-row" });

    act(() => row.props.onPointerEnter());
    act(() => {
      vi.advanceTimersByTime(1_000);
    });

    const preview = tree.root.findByProps({ role: "dialog" });
    expect(preview.findByType("header").findByType("strong").children.join("")).toBe("Existing session");
    expect(preview.findAllByType("strong").map((item) => item.children.join(""))).toEqual(["Existing session", "/workspace/OpenAIDE"]);
    expect(onOpenNativeSession).not.toHaveBeenCalled();

    const helpTrigger = preview.findByProps({ "aria-label": "What loading from Codex means" });
    expect(helpTrigger.children.join("")).toBe("From Codex");
    expect(preview.findByProps({ className: "task-preview-source-action" }).children.join(""))
      .toBe("· Open to load");
    act(() => helpTrigger.props.onPointerEnter());
    expect(preview.findByProps({ role: "tooltip" }).children.join(" ")).toContain(
      "Opening it creates an OpenAIDE task and loads its message history.",
    );

    act(() => tree.root.findAllByProps({ "aria-label": "Open Existing session" })[0].props.onClick());
    expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(0);
    expect(onOpenNativeSession).toHaveBeenCalledWith(session);
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("disables listed session actions while adoption is pending", () => {
    const tree = render(
      <SidebarNativeSessionRow
        nativeSessionAgentId="codex"
        nativeSessionAgentName="Codex"
        nativeSessionsAdoptingSessionId="session_1"
        onOpenNativeSession={vi.fn()}
        session={nativeSession({ session_id: "session_1" })}
      />,
    );

    expect(tree.root.findAllByType("button").map((button) => button.props.disabled)).toEqual([true, true, true]);
    expect(tree.root.findAllByType("button")[1].props.title).toBe("Opening task");
    expect(tree.root.findByProps({ className: "task-trailing-indicator" }).props["aria-label"]).toBe("Opening task");
  });

  it("keeps other Native Sessions clickable while one adoption is pending", () => {
    const onOpenNativeSession = vi.fn();
    const session = nativeSession({ session_id: "session_2", title: "Another session" });
    const tree = render(
      <SidebarNativeSessionRow
        nativeSessionAgentId="codex"
        nativeSessionAgentName="Codex"
        nativeSessionsAdoptingSessionId="session_1"
        onOpenNativeSession={onOpenNativeSession}
        session={session}
      />,
    );

    const buttons = tree.root.findAllByType("button");
    expect(buttons.map((button) => button.props.disabled)).toEqual([false, false, false]);

    act(() => buttons[0].props.onClick());
    expect(onOpenNativeSession).toHaveBeenCalledWith(session);
  });
});

describe("Sidebar", () => {
  it("marks hidden navigation inert and outside the accessibility tree", () => {
    const tree = render(
      <Sidebar
        hiddenFromAccessibility
        nativeSessions={nativeSessions()}
        showArchived={false}
        tasks={[]}
        {...sidebarCallbacks()}
      />,
    );

    const sidebar = tree.root.findByType("aside");
    expect(sidebar.props["aria-hidden"]).toBe(true);
    expect(sidebar.props.inert).toBe(true);
  });

  it("can expose the task navigation as a modal drawer", () => {
    const tree = render(
      <Sidebar
        modal
        nativeSessions={nativeSessions()}
        showArchived={false}
        tasks={[]}
        {...sidebarCallbacks()}
      />,
    );

    const sidebar = tree.root.findByType("aside");
    expect(sidebar.props.role).toBe("dialog");
    expect(sidebar.props["aria-modal"]).toBe(true);
    expect(sidebar.props["aria-label"]).toBe("Task navigation");
  });

  it("hides native sessions and shows archive empty copy in archive mode", () => {
    const tree = render(
      <Sidebar
        nativeSessions={nativeSessions({ items: [nativeSession({ session_id: "session_1", title: "Existing" })] })}
        showArchived={true}
        tasks={[]}
        {...sidebarCallbacks()}
      />,
    );

    expect(tree.root.findByProps({ className: "empty-list" }).children).toEqual([
      "Archive is empty. Archived tasks will appear here.",
    ]);
    expect(tree.root.findAllByProps({ className: "task-row external-session-row" })).toHaveLength(0);
  });

  it("does not expose Tasks pagination in Archive", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject
        nativeSessions={nativeSessions({ hasMoreProjectIds: ["project_1"] })}
        projects={[{ projectId: "project_1", label: "OpenAIDE" }]}
        showArchived
        tasks={[]}
      />,
    );

    expect(tree.root.findAllByProps({ className: "project-task-more" })).toHaveLength(0);
  });

  it("renders native-session errors without hiding valid rows", () => {
    const onRecoverNativeSessions = vi.fn();
    const tree = render(
      <Sidebar
        nativeSessions={nativeSessions({
          error: "Codex history unavailable",
          recoveryKind: "nodeJsRequired",
          items: [nativeSession({ session_id: "session_1", title: "Existing" })],
        })}
        onRecoverNativeSessions={onRecoverNativeSessions}
        showArchived={false}
        tasks={[]}
        {...sidebarCallbacks()}
      />,
    );

    expect(tree.root.findByProps({ className: "native-session-recovery" }).findByType("span").children)
      .toEqual(["Codex history unavailable"]);
    act(() => tree.root.findByProps({ className: "native-session-recovery" }).findByType("button").props.onClick());
    expect(onRecoverNativeSessions).toHaveBeenCalledWith("nodeJsRequired");
    expect(tree.root.findAllByProps({ className: "task-row external-session-row" })).toHaveLength(1);
  });

  it("refreshes external sessions from the sidebar header", () => {
    const onLoadNativeSessions = vi.fn();
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        nativeSessions={nativeSessions()}
        onLoadNativeSessions={onLoadNativeSessions}
        showArchived={false}
        tasks={[]}
      />,
    );

    act(() => tree.root.findByProps({ "aria-label": "Refresh tasks" }).props.onClick());

    expect(onLoadNativeSessions).toHaveBeenCalledWith();
  });

  it("keeps the native-session refresh action adjacent to the Tasks title", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        nativeSessions={nativeSessions()}
        showArchived={false}
        tasks={[]}
      />,
    );

    const header = tree.root.findByProps({ className: "task-section-head" });
    const [title, tools] = header.findAllByType("span");

    expect(title.props.className).toBe("task-section-title");
    expect(title.children.join("")).toBe("Tasks");
    expect(tools.props.className).toBe("task-section-tools");
    expect(tools.findByProps({ "aria-label": "Refresh tasks" })).toBeDefined();
  });

  it("marks Settings as the sole current navigation destination", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        activeTaskId="task_1"
        nativeSessions={nativeSessions()}
        settingsActive
        showArchived={false}
        tasks={[task({ task_id: "task_1" })]}
      />,
    );

    const settings = tree.root.findByProps({ "aria-current": "page" });
    expect(settings.props.className).toContain("settings-button selected");
  });

  it("labels native-session refresh in the sidebar header", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        nativeSessions={nativeSessions({ loading: true })}
        showArchived={false}
        tasks={[]}
      />,
    );

    expect(tree.root.findByProps({ className: "task-section-head" }).findByType("small").children.join("")).toBe(
      "Refreshing tasks",
    );
    expect(tree.root.findByProps({ "aria-label": "Refresh tasks" }).props.disabled).toBe(true);
    expect(tree.root.findByProps({ "aria-label": "Refresh tasks" }).props.className).toContain("refreshing");
  });

  it("hides the external-session refresh control in archive mode", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        nativeSessions={nativeSessions()}
        showArchived={true}
        tasks={[]}
      />,
    );

    expect(tree.root.findAllByProps({ "aria-label": "Refresh tasks" })).toHaveLength(0);
  });

  it("opens Archive as a secondary destination instead of an equal tab", () => {
    const onToggleArchived = vi.fn();
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        nativeSessions={nativeSessions()}
        onToggleArchived={onToggleArchived}
        showArchived={false}
        tasks={[]}
      />,
    );

    expect(tree.root.findAllByProps({ className: "task-mode-tabs" })).toHaveLength(0);
    const archive = tree.root.findByProps({ className: "archive-navigation" });

    act(() => archive.props.onClick());

    expect(onToggleArchived).toHaveBeenCalledTimes(1);
  });

  it("renders Archive as a visibly read-only secondary destination", () => {
    const onToggleArchived = vi.fn();
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        nativeSessions={nativeSessions()}
        onToggleArchived={onToggleArchived}
        showArchived={true}
        tasks={[]}
      />,
    );

    expect(tree.root.findByProps({ className: "archive-section-head" }).findByType("strong").children).toEqual(["Archive"]);
    expect(tree.root.findByType("small").children).toEqual(["Read-only tasks"]);
    expect(tree.root.findAllByProps({ children: "New task" })).toHaveLength(0);
    act(() => tree.root.findByProps({ "aria-label": "Back to tasks" }).props.onClick());
    expect(onToggleArchived).toHaveBeenCalledTimes(1);
  });

  it("uses compact agent icons without repeating agent names", () => {
    const sameAgent = render(
      <Sidebar
        {...sidebarCallbacks()}
        nativeSessions={nativeSessions()}
        showArchived={false}
        tasks={[task({ task_id: "task_1" }), task({ task_id: "task_2" })]}
      />,
    );
    expect(sameAgent.root.findAllByProps({ className: "task-trailing-agent-name" })).toHaveLength(0);
    expect(sameAgent.root.findAllByProps({ className: "task-agent-icon" })).toHaveLength(2);
    expect(sameAgent.root.findAllByProps({ className: "task-meta-age" })).toHaveLength(2);

    const mixedAgent = render(
      <Sidebar
        {...sidebarCallbacks()}
        nativeSessions={nativeSessions()}
        showArchived={false}
        tasks={[
          task({ task_id: "task_1", agent_name: "Codex" }),
          task({ task_id: "task_2", agent_id: "gpt", agent_name: "GPT" }),
        ]}
      />,
    );
    expect(mixedAgent.root.findAllByProps({ className: "task-trailing-agent-name" })).toHaveLength(0);
  });

  it("renders task-list errors instead of empty copy", () => {
    const tree = render(
      <Sidebar
        nativeSessions={nativeSessions()}
        showArchived={false}
        taskListError="Unable to load tasks from App Server"
        tasks={[]}
        {...sidebarCallbacks()}
      />,
    );

    expect(tree.root.findAllByProps({ className: "empty-list" }).map((item) => item.children.join(""))).toEqual([
      "Unable to load tasks from App Server",
    ]);
  });

  it("loads the next task-history page with the current cursor", () => {
    const onLoadNativeSessions = vi.fn();
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        nativeSessions={nativeSessions({ nextCursor: "cursor_2" })}
        onLoadNativeSessions={onLoadNativeSessions}
        showArchived={false}
        tasks={[]}
      />,
    );

    act(() => tree.root.findByProps({ className: "session-more" }).props.onClick());

    expect(tree.root.findByProps({ className: "session-more" }).children.join("")).toBe("Load more tasks");
    expect(onLoadNativeSessions).toHaveBeenCalledWith("cursor_2");
  });

  it("shows task loading instead of a false empty state before navigation arrives", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        loadingTasks
        nativeSessions={nativeSessions()}
        showArchived={false}
        tasks={[]}
      />,
    );

    expect(tree.root.findByProps({ className: "empty-list" }).children.join("")).toBe("Loading tasks.");
  });

  it("keeps an existing Task list free of loading copy while another Task opens", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        loadingTasks
        nativeSessions={nativeSessions()}
        showArchived={false}
        tasks={[task({ task_id: "task-existing" })]}
      />,
    );

    expect(tree.root.findAllByProps({ className: "empty-list" })).toHaveLength(0);
    expect(tree.root.findAllByType(SidebarTaskRow)).toHaveLength(1);
  });

  it("labels task-history pagination as search-specific while filtering", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        nativeSessions={nativeSessions({ nextCursor: "cursor_2" })}
        searchQuery="missing task"
        showArchived={false}
        tasks={[]}
      />,
    );

    expect(tree.root.findByProps({ className: "session-more" }).children.join("")).toBe("Search more tasks");
  });

  it("sorts flat task and native-session rows together by last activity", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        nativeSessions={nativeSessions({
          items: [
            nativeSession({
              session_id: "session_recent",
              title: "Recent session",
              last_activity: "2026-05-22T00:03:00.000Z",
              updated_at: "2026-05-22T00:03:00.000Z",
            }),
            nativeSession({
              session_id: "session_old",
              title: "Old session",
              last_activity: "2026-05-22T00:01:00.000Z",
              updated_at: "2026-05-22T00:01:00.000Z",
            }),
          ],
        })}
        showArchived={false}
        tasks={[
          task({
            task_id: "task_middle",
            title: "Middle task",
            last_activity: "2026-05-22T00:02:00.000Z",
          }),
          task({
            task_id: "task_newest",
            title: "Newest task",
            last_activity: "2026-05-22T00:04:00.000Z",
          }),
        ]}
      />,
    );

    expect(rowTitles(tree)).toEqual(["Newest task", "Recent session", "Middle task", "Old session"]);
  });

  it("puts in-progress flat task rows before newer idle rows and native sessions", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        nativeSessions={nativeSessions({
          items: [
            nativeSession({
              session_id: "session_recent",
              title: "Recent session",
              last_activity: "2026-05-22T00:05:00.000Z",
              updated_at: "2026-05-22T00:05:00.000Z",
            }),
          ],
        })}
        showArchived={false}
        tasks={[
          task({
            task_id: "task_idle_recent",
            title: "Recent idle task",
            last_activity: "2026-05-22T00:04:00.000Z",
          }),
          task({
            task_id: "task_active_old",
            status: "active",
            title: "Older active task",
            last_activity: "2026-05-22T00:01:00.000Z",
          }),
        ]}
      />,
    );

    expect(rowTitles(tree)).toEqual(["Older active task", "Recent session", "Recent idle task"]);
  });

  it("sorts rows by parsed activity time when timestamp formats differ", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        nativeSessions={nativeSessions({
          items: [
            nativeSession({
              session_id: "session_old_iso",
              title: "Old ISO session",
              last_activity: "2026-06-30T00:00:00.000Z",
              updated_at: "2026-06-30T00:00:00.000Z",
            }),
          ],
        })}
        showArchived={false}
        tasks={[
          task({
            task_id: "task_recent_epoch",
            title: "Recent epoch task",
            last_activity: "1782781200000",
          }),
        ]}
      />,
    );

    expect(rowTitles(tree)).toEqual(["Recent epoch task", "Old ISO session"]);
  });

  it("does not treat Task record update time as Task activity", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        nativeSessions={nativeSessions()}
        showArchived={false}
        tasks={[
          task({
            task_id: "task_missing_activity",
            title: "Missing activity",
            last_activity: undefined as never,
            updated_at: "2026-05-22T00:05:00.000Z",
          }),
          task({
            task_id: "task_old",
            title: "Old task",
            last_activity: "2026-05-22T00:01:00.000Z",
            updated_at: "2026-05-22T00:01:00.000Z",
          }),
        ]}
      />,
    );

    expect(rowTitles(tree)).toEqual(["Old task", "Missing activity"]);
  });

  it("disables load-more while native sessions are loading or adopting", () => {
    const loadingTree = render(
      <Sidebar
        nativeSessions={nativeSessions({ loading: true, nextCursor: "cursor_2" })}
        showArchived={false}
        tasks={[]}
        {...sidebarCallbacks()}
      />,
    );
    const adoptingTree = render(
      <Sidebar
        nativeSessions={nativeSessions({ adoptingSessionId: "session_1", nextCursor: "cursor_2" })}
        showArchived={false}
        tasks={[]}
        {...sidebarCallbacks()}
      />,
    );

    expect(loadingTree.root.findByProps({ className: "session-more" }).props.disabled).toBe(true);
    expect(adoptingTree.root.findByProps({ className: "session-more" }).props.disabled).toBe(true);
  });

  it("keeps project groups expanded by default, resets on re-expand, and reveals ten more rows", () => {
    const tasks = Array.from({ length: 32 }, (_, index) =>
      task({
        task_id: `task_${index + 1}`,
        project_id: "project_1",
        project_label: "OpenAIDE",
        title: `Task ${index + 1}`,
      }),
    );
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        nativeSessions={nativeSessions()}
        projects={[{ projectId: "project_1", label: "OpenAIDE" }]}
        showArchived={false}
        tasks={tasks}
      />,
    );

    expect(taskRows(tree)).toHaveLength(20);
    expect(tree.root.findByProps({ className: "project-task-group-toggle" }).props["aria-expanded"]).toBe(true);

    act(() => tree.root.findByProps({ className: "project-task-group-toggle" }).props.onClick());
    expect(tree.root.findByProps({ className: "project-task-group-toggle" }).props["aria-expanded"]).toBe(false);
    expect(tree.root.findByProps({ className: "project-task-group-rows collapsed" }).props).toMatchObject({
      "aria-hidden": true,
      inert: true,
    });

    act(() => tree.root.findByProps({ className: "project-task-group-toggle" }).props.onClick());
    expect(taskRows(tree)).toHaveLength(20);
    expect(tree.root.findByProps({ className: "project-task-more" }).children.join("")).toBe("Load more");
    act(() => tree.root.findByProps({ className: "project-task-more" }).props.onClick());

    expect(taskRows(tree)).toHaveLength(30);
    expect(tree.root.findByProps({ className: "project-task-more" }).children.join("")).toBe("Load more");
    act(() => tree.root.findByProps({ className: "project-task-more" }).props.onClick());

    expect(taskRows(tree)).toHaveLength(32);
    expect(tree.root.findAllByProps({ className: "project-task-more" })).toHaveLength(0);

    act(() => tree.root.findByProps({ className: "project-task-group-toggle" }).props.onClick());
    act(() => tree.root.findByProps({ className: "project-task-group-toggle" }).props.onClick());
    expect(taskRows(tree)).toHaveLength(20);
  });

  it("hides the active task when its project group is collapsed", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        activeTaskId="task_6"
        groupByProject={true}
        nativeSessions={nativeSessions()}
        projects={[{ projectId: "project_1", label: "OpenAIDE" }]}
        showArchived={false}
        tasks={Array.from({ length: 6 }, (_, index) =>
          task({
            task_id: `task_${index + 1}`,
            project_id: "project_1",
            project_label: "OpenAIDE",
            title: `Task ${index + 1}`,
            last_activity: `2026-05-22T00:0${index}:00.000Z`,
          }),
        )}
      />,
    );

    act(() => tree.root.findByProps({ className: "project-task-group-toggle" }).props.onClick());

    expect(tree.root.findByProps({ className: "project-task-group-toggle" }).props["aria-expanded"]).toBe(false);
    expect(tree.root.findByProps({ className: "project-task-group-rows collapsed" }).props).toMatchObject({
      "aria-hidden": true,
      inert: true,
    });
  });

  it("uses project pagination instead of global native-session pagination in grouped mode", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        maxTasksPerProject={15}
        nativeSessionProjectId="project_1"
        nativeSessions={nativeSessions({
          items: [nativeSession({ session_id: "session_1", title: "Existing native session" })],
          nextCursor: "cursor_2",
        })}
        projects={[{ projectId: "project_1", label: "OpenAIDE" }]}
        showArchived={false}
        tasks={[]}
      />,
    );

    expect(tree.root.findAllByProps({ className: "session-more" })).toHaveLength(0);

    act(() => tree.root.findByProps({ className: "project-task-group-toggle" }).props.onClick());

    expect(tree.root.findByProps({ className: "project-task-group-rows collapsed" }).props["aria-hidden"]).toBe(true);
    expect(tree.root.findAllByProps({ className: "session-more" })).toHaveLength(0);
  });

  it("renders backend-known projects even when they do not have tasks yet", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        nativeSessions={nativeSessions()}
        projects={[
          { projectId: "project_1", label: "OpenAIDE" },
          { projectId: "project_2", label: "No tasks yet" },
        ]}
        showArchived={false}
        tasks={[]}
      />,
    );

    expect(tree.root.findAllByProps({ className: "empty-list" })).toHaveLength(0);
    expect(
      tree.root.findAllByProps({ className: "project-task-group" }).map((group) => group.props["aria-label"]),
    ).toEqual(["No tasks yet", "OpenAIDE"]);
    expect(tree.root.findAllByProps({ className: "project-task-group-new" })).toHaveLength(0);
  });

  it("shows the first five workspace groups and reveals more workspaces in batches", () => {
    const projects = Array.from({ length: 7 }, (_, index) => ({
      projectId: `project_${index + 1}`,
      label: `Project ${index + 1}`,
    }));
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        maxVisibleProjects={5}
        nativeSessions={nativeSessions()}
        projects={projects}
        showArchived={false}
        tasks={[]}
      />,
    );

    expect(tree.root.findAllByProps({ className: "project-task-group" })).toHaveLength(5);
    expect(tree.root.findByProps({ className: "project-more" }).children.join("")).toBe("Show 2 more workspaces");

    act(() => tree.root.findByProps({ className: "project-more" }).props.onClick());

    expect(tree.root.findAllByProps({ className: "project-task-group" })).toHaveLength(7);
    expect(tree.root.findAllByProps({ className: "project-more" })).toHaveLength(0);
  });

  it("filters empty project groups by project label while searching", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        nativeSessions={nativeSessions()}
        projects={[
          { projectId: "project_1", label: "OpenAIDE" },
          { projectId: "project_2", label: "Receipt Splitter" },
        ]}
        searchQuery="receipt"
        showArchived={false}
        tasks={[]}
      />,
    );

    expect(
      tree.root.findAllByProps({ className: "project-task-group" }).map((group) => group.props["aria-label"]),
    ).toEqual(["Receipt Splitter"]);
    expect(tree.root.findAllByProps({ className: "empty-list" })).toHaveLength(0);
  });

  it("shows matching task rows while a project group is collapsed during search", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        nativeSessions={nativeSessions()}
        projects={[{ projectId: "project_1", label: "OpenAIDE" }]}
        showArchived={false}
        tasks={[
          task({
            task_id: "task_1",
            project_id: "project_1",
            project_label: "OpenAIDE",
            title: "QA smoke test",
          }),
        ]}
      />,
    );

    act(() => tree.root.findByProps({ className: "project-task-group-toggle" }).props.onClick());
    expect(tree.root.findByProps({ className: "project-task-group-toggle" }).props["aria-expanded"]).toBe(false);
    expect(tree.root.findByProps({ className: "project-task-group-rows collapsed" }).props["aria-hidden"]).toBe(true);

    act(() => tree.update(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        nativeSessions={nativeSessions()}
        projects={[{ projectId: "project_1", label: "OpenAIDE" }]}
        searchQuery="qa smoke"
        showArchived={false}
        tasks={[
          task({
            task_id: "task_1",
            project_id: "project_1",
            project_label: "OpenAIDE",
            title: "QA smoke test",
          }),
        ]}
      />,
    ));

    expect(taskRows(tree).map((row) => row.findByProps({ className: "task-title" }).children.join(""))).toEqual([
      "QA smoke test",
    ]);
    expect(tree.root.findByProps({ className: "project-task-group-toggle" }).props["aria-expanded"]).toBe(true);

    act(() => tree.update(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        nativeSessions={nativeSessions()}
        projects={[{ projectId: "project_1", label: "OpenAIDE" }]}
        showArchived={false}
        tasks={[
          task({
            task_id: "task_1",
            project_id: "project_1",
            project_label: "OpenAIDE",
            title: "QA smoke test",
          }),
        ]}
      />,
    ));

    expect(tree.root.findByProps({ className: "project-task-group-toggle" }).props["aria-expanded"]).toBe(false);
    expect(tree.root.findByProps({ className: "project-task-group-rows collapsed" }).props["aria-hidden"]).toBe(true);
  });

  it("explains when search keeps the selected task visible outside the match set", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        activeTaskId="task_1"
        groupByProject={true}
        nativeSessions={nativeSessions()}
        projects={[{ projectId: "project_1", label: "OpenAIDE" }]}
        searchQuery="billing"
        showArchived={false}
        tasks={[
          task({
            task_id: "task_1",
            project_id: "project_1",
            project_label: "OpenAIDE",
            title: "Selected task",
          }),
        ]}
      />,
    );

    expect(tree.root.findByProps({ className: "search-context-note" }).children.join("")).toBe(
      "Selected task is shown outside the search results.",
    );
    expect(rowTitles(tree)).toEqual(["Selected task"]);
  });

  it("renders native sessions inside the selected project group in grouped mode", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        maxTasksPerProject={15}
        nativeSessionProjectId="project_1"
        nativeSessions={nativeSessions({
          items: [nativeSession({ session_id: "session_1", title: "Current session" })],
        })}
        projects={[{ projectId: "project_1", label: "OpenAIDE" }]}
        showArchived={false}
        tasks={[]}
      />,
    );

    const group = tree.root.findByProps({ className: "project-task-group" });
    expect(group.props["aria-label"]).toBe("OpenAIDE");
    expect(group.findByProps({ className: "project-task-group-counts" }).children.join("")).toBe("1 task");
    expect(group.findAllByProps({ className: "task-row external-session-row" })).toHaveLength(1);
    expect(tree.root.findAllByProps({ className: "task-row external-session-row" })).toHaveLength(1);

    act(() => tree.root.findByProps({ className: "project-task-group-toggle" }).props.onClick());

    expect(group.findByProps({ className: "project-task-group-rows collapsed" }).props["aria-hidden"]).toBe(true);
  });

  it("keeps older loaded native sessions reachable when recent local tasks fill a project group", () => {
    const tasks = Array.from({ length: 16 }, (_, index) =>
      task({
        task_id: `task_${index + 1}`,
        project_id: "project_1",
        project_label: "OpenAIDE",
        title: `Task ${index + 1}`,
        last_activity: `2026-06-30T00:${String(index).padStart(2, "0")}:00.000Z`,
      }),
    );
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        maxTasksPerProject={15}
        nativeSessionProjectId="project_1"
        nativeSessions={nativeSessions({
          items: [
            nativeSession({
              session_id: "session_1",
              title: "Existing native session",
              updated_at: "2026-06-29T00:00:00.000Z",
            }),
          ],
        })}
        projects={[{ projectId: "project_1", label: "OpenAIDE" }]}
        showArchived={false}
        tasks={tasks}
      />,
    );

    expect(localTaskRows(tree)).toHaveLength(15);
    expect(tree.root.findByProps({ className: "project-task-more" }).children.join("")).toBe("Load more");
    expect(tree.root.findAllByProps({ className: "task-row external-session-row" })).toHaveLength(0);

    act(() => tree.root.findByProps({ className: "project-task-more" }).props.onClick());

    expect(tree.root.findAllByProps({ className: "task-row external-session-row" })).toHaveLength(1);
    expect(localTaskRows(tree)).toHaveLength(16);
  });

  it("orders project groups by the newest task or native-session activity", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        nativeSessionProjectId="project_1"
        nativeSessions={nativeSessions({
          items: [
            nativeSession({
              session_id: "session_newest",
              title: "Newest session",
              last_activity: "2026-05-22T00:05:00.000Z",
              updated_at: "2026-05-22T00:05:00.000Z",
            }),
          ],
        })}
        projects={[
          { projectId: "project_1", label: "OpenAIDE" },
          { projectId: "project_2", label: "Other" },
        ]}
        showArchived={false}
        tasks={[
          task({
            task_id: "task_other",
            project_id: "project_2",
            project_label: "Other",
            title: "Other task",
            last_activity: "2026-05-22T00:04:00.000Z",
          }),
        ]}
      />,
    );

    expect(
      tree.root.findAllByProps({ className: "project-task-group" }).map((group) => group.props["aria-label"]),
    ).toEqual(["OpenAIDE", "Other"]);
  });

  it("puts project groups with in-progress tasks before newer idle groups", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        nativeSessions={nativeSessions()}
        projects={[
          { projectId: "project_1", label: "OpenAIDE" },
          { projectId: "project_2", label: "Other" },
        ]}
        showArchived={false}
        tasks={[
          task({
            task_id: "task_active_old",
            project_id: "project_1",
            project_label: "OpenAIDE",
            status: "active",
            title: "Older active task",
            last_activity: "2026-05-22T00:01:00.000Z",
          }),
          task({
            task_id: "task_idle_recent",
            project_id: "project_2",
            project_label: "Other",
            title: "Recent idle task",
            last_activity: "2026-05-22T00:05:00.000Z",
          }),
        ]}
      />,
    );

    expect(
      tree.root.findAllByProps({ className: "project-task-group" }).map((group) => group.props["aria-label"]),
    ).toEqual(["OpenAIDE", "Other"]);
  });

  it("sorts active task rows by last activity instead of creation time", () => {
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        nativeSessions={nativeSessions()}
        projects={[{ projectId: "project_1", label: "OpenAIDE" }]}
        showArchived={false}
        tasks={[
          task({
            task_id: "task_earlier",
            project_id: "project_1",
            project_label: "OpenAIDE",
            status: "active",
            title: "Earlier active",
            created_at: "2026-07-02T10:00:00.000Z",
            last_activity: "2026-07-02T10:10:00.000Z",
          }),
          task({
            task_id: "task_later",
            project_id: "project_1",
            project_label: "OpenAIDE",
            status: "active",
            title: "Later active",
            created_at: "2026-07-02T10:05:00.000Z",
            last_activity: "2026-07-02T10:05:00.000Z",
          }),
        ]}
      />,
    );

    expect(rowTitles(tree)).toEqual(["Earlier active", "Later active"]);
  });

  it("uses the primary new task action instead of per-project create controls", () => {
    const onNewTask = vi.fn();
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        nativeSessions={nativeSessions()}
        onNewTask={onNewTask}
        projects={[{ projectId: "project_1", label: "OpenAIDE" }]}
        showArchived={false}
        tasks={[
          task({
            task_id: "task_1",
            project_id: "project_1",
            project_label: "OpenAIDE",
          }),
        ]}
      />,
    );

    expect(tree.root.findAllByProps({ className: "project-task-group-new" })).toHaveLength(0);
    act(() => tree.root.findAllByType("button")[0].props.onClick());

    expect(onNewTask).toHaveBeenCalledWith();
  });

  it("limits loaded project task and native-session rows together", () => {
    const onLoadNativeSessions = vi.fn();
    const sessions = Array.from({ length: 16 }, (_, index) =>
      nativeSession({
        session_id: `session_${index + 1}`,
        title: `Session ${index + 1}`,
        updated_at: `2026-05-22T00:${String(index).padStart(2, "0")}:00.000Z`,
      }),
    );
    const tree = render(
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        maxTasksPerProject={15}
        nativeSessionProjectId="project_1"
        nativeSessions={nativeSessions({
          items: sessions,
          hasMoreProjectIds: ["project_1"],
        })}
        onLoadNativeSessions={onLoadNativeSessions}
        projects={[{ projectId: "project_1", label: "OpenAIDE" }]}
        showArchived={false}
        tasks={[
          task({
            task_id: "task_1",
            project_id: "project_1",
            project_label: "OpenAIDE",
            title: "Recent task",
            last_activity: "2026-05-22T00:06:00.000Z",
          }),
        ]}
      />,
    );

    expect(localTaskRows(tree)).toHaveLength(1);
    expect(tree.root.findAllByProps({ className: "task-row external-session-row" })).toHaveLength(14);
    expect(tree.root.findByProps({ className: "project-task-more" }).children.join("")).toBe("Load more");
    expect(taskRows(tree)).toHaveLength(15);

    act(() => tree.root.findByProps({ className: "project-task-more" }).props.onClick());

    expect(localTaskRows(tree)).toHaveLength(1);
    expect(tree.root.findAllByProps({ className: "task-row external-session-row" })).toHaveLength(16);
    expect(tree.root.findByProps({ className: "project-task-more" }).children.join(""))
      .toBe("Load more");
    expect(taskRows(tree)).toHaveLength(17);
    expect(onLoadNativeSessions).toHaveBeenCalledWith(undefined, "project_1", 17);
  });

  it("reveals exactly the numeric task count when a prefetched page arrives", () => {
    const onLoadNativeSessions = vi.fn();
    const sessions = Array.from({ length: 31 }, (_, index) =>
      nativeSession({
        session_id: `session_${index + 1}`,
        title: `Session ${index + 1}`,
        updated_at: `2026-05-22T00:${String(index).padStart(2, "0")}:00.000Z`,
      }),
    );
    const sidebar = (items: AgentListedSession[], nextCursor: string) => (
      <Sidebar
        {...sidebarCallbacks()}
        groupByProject={true}
        maxTasksPerProject={15}
        nativeSessionProjectId="project_1"
        nativeSessions={nativeSessions({ items, nextCursor })}
        onLoadNativeSessions={onLoadNativeSessions}
        projects={[{ projectId: "project_1", label: "OpenAIDE" }]}
        showArchived={false}
        tasks={[
          task({
            task_id: "task_1",
            project_id: "project_1",
            project_label: "OpenAIDE",
            title: "Recent task",
            last_activity: "2026-05-22T00:40:00.000Z",
          }),
        ]}
      />
    );
    const tree = render(sidebar(sessions.slice(0, 16), "cursor_2"));

    expect(taskRows(tree)).toHaveLength(15);
    expect(tree.root.findByProps({ className: "project-task-more" }).children.join(""))
      .toBe("Load more");

    act(() => tree.root.findByProps({ className: "project-task-more" }).props.onClick());
    act(() => tree.update(sidebar(sessions, "cursor_3")));

    expect(taskRows(tree)).toHaveLength(17);
    expect(tree.root.findByProps({ className: "project-task-more" }).children.join(""))
      .toBe("Load more");
  });
});

function taskRows(tree: ReturnType<typeof render>) {
  return tree.root.findAll((node) =>
    node.props.role === "listitem" &&
    typeof node.props.className === "string" &&
    node.props.className.includes("task-row"),
  );
}

function localTaskRows(tree: ReturnType<typeof render>) {
  return taskRows(tree).filter((node) => !node.props.className.includes("external-session-row"));
}

function rowTitles(tree: ReturnType<typeof render>) {
  return taskRows(tree).map((row) => row.findByProps({ className: "task-title" }).children.join(""));
}

function render(element: React.ReactElement, options?: Parameters<typeof create>[1]) {
  let tree: ReturnType<typeof create> | undefined;
  act(() => {
    tree = create(element, options);
  });
  return tree!;
}

function sidebarCallbacks() {
  return {
    nativeSessionAgentId: "codex",
    nativeSessionAgentName: "Codex",
    onArchiveTask: vi.fn(),
    onLoadNativeSessions: vi.fn(),
    onNewTask: vi.fn(),
    onOpenNativeSession: vi.fn(),
    onOpenTask: vi.fn(),
    onRestoreTask: vi.fn(),
    onSearchChange: vi.fn(),
    onSettings: vi.fn(),
    onToggleArchived: vi.fn(),
    searchQuery: "",
  };
}

function nativeSessions(overrides: Partial<NativeSessionsState> = {}): NativeSessionsState {
  return {
    items: [],
    loaded: true,
    loading: false,
    ...overrides,
  };
}

function nativeSession(overrides: Partial<AgentListedSession> = {}): AgentListedSession {
  return {
    cwd: "/workspace",
    session_id: "session",
    last_activity: "2026-05-22T00:00:00.000Z",
    updated_at: "2026-05-22T00:00:00.000Z",
    ...overrides,
  };
}

function task(overrides: Partial<TaskSummary> = {}): TaskSummary {
  return {
    agent_id: "codex",
    agent_name: "Codex",
    created_at: "2026-05-22T00:00:00.000Z",
    isolation: "local",
    last_activity: "2026-05-22T00:00:00.000Z",
    message_history_version: 1,
    has_messages: true,
    status: "inactive",
    task_id: "task",
    task_version: 1,
    title: "Task",
    unread: false,
    updated_at: "2026-05-22T00:00:00.000Z",
    workspace_root: "/workspace",
    ...overrides,
  };
}
