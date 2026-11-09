// timing-file: mocked — long-press hold time runs on Vitest fake timers.
import { act, create } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskSummary } from "@openaide/app-shell-contracts";
import type { NativeSessionsState } from "../state/store";
import { Sidebar } from "./Sidebar";
import { SidebarTaskRow } from "./SidebarTaskRow";
import { LONG_PRESS_DELAY_MS, LONG_PRESS_SLOP_PX } from "./useTouchLongPress";
import { PULL_REFRESH_THRESHOLD_PX } from "./usePullToRefresh";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("task row long press", () => {
  it("opens the row's actions after a still touch and swallows the release tap", () => {
    const onOpenTask = vi.fn();
    const tree = renderRow(onOpenTask);
    const row = tree.root.findByProps({ role: "listitem" });

    act(() => row.props.onPointerDown(pointer("touch", 20, 20)));
    expect(menuTrigger(tree).props["aria-expanded"]).toBe(false);
    act(() => { vi.advanceTimersByTime(LONG_PRESS_DELAY_MS); });
    expect(menuTrigger(tree).props["aria-expanded"]).toBe(true);

    const click = { preventDefault: vi.fn(), stopPropagation: vi.fn() };
    act(() => row.props.onPointerUp(pointer("touch", 20, 20)));
    act(() => row.props.onClickCapture(click));
    expect(click.stopPropagation).toHaveBeenCalled();
    expect(onOpenTask).not.toHaveBeenCalled();
  });

  it("leaves mouse presses and moving touches to their ordinary meaning", () => {
    const tree = renderRow(vi.fn());
    const row = tree.root.findByProps({ role: "listitem" });

    act(() => row.props.onPointerDown(pointer("mouse", 20, 20)));
    act(() => { vi.advanceTimersByTime(LONG_PRESS_DELAY_MS); });
    expect(menuTrigger(tree).props["aria-expanded"]).toBe(false);

    act(() => row.props.onPointerDown(pointer("touch", 20, 20)));
    act(() => row.props.onPointerMove(pointer("touch", 20, 20 + LONG_PRESS_SLOP_PX + 1)));
    act(() => { vi.advanceTimersByTime(LONG_PRESS_DELAY_MS); });
    expect(menuTrigger(tree).props["aria-expanded"]).toBe(false);

    const click = { preventDefault: vi.fn(), stopPropagation: vi.fn() };
    act(() => row.props.onClickCapture(click));
    expect(click.stopPropagation).not.toHaveBeenCalled();
  });
});

describe("task list pull to refresh", () => {
  it("refreshes when a pull from the top passes the threshold", () => {
    const onLoadNativeSessions = vi.fn();
    const tree = renderSidebar(onLoadNativeSessions);
    const list = tree.root.findByProps({ role: "list" });

    act(() => list.props.onTouchStart(touch(100, 100)));
    act(() => list.props.onTouchMove(touch(100, 100 + PULL_REFRESH_THRESHOLD_PX / 2)));
    act(() => list.props.onTouchEnd());
    expect(onLoadNativeSessions).not.toHaveBeenCalled();

    act(() => list.props.onTouchStart(touch(100, 100)));
    act(() => list.props.onTouchMove(touch(100, 110)));
    act(() => list.props.onTouchMove(touch(100, 100 + PULL_REFRESH_THRESHOLD_PX + 10)));
    expect(tree.root.findByProps({ className: "task-pull-refresh" }).props["data-armed"]).toBe(true);
    act(() => list.props.onTouchEnd());
    expect(onLoadNativeSessions).toHaveBeenCalledTimes(1);
  });

  it("leaves a horizontal drag to the drawer swipe", () => {
    const onLoadNativeSessions = vi.fn();
    const tree = renderSidebar(onLoadNativeSessions);
    const list = tree.root.findByProps({ role: "list" });

    act(() => list.props.onTouchStart(touch(100, 100)));
    act(() => list.props.onTouchMove(touch(40, 104)));
    act(() => list.props.onTouchMove(touch(40, 100 + PULL_REFRESH_THRESHOLD_PX + 10)));
    act(() => list.props.onTouchEnd());
    expect(onLoadNativeSessions).not.toHaveBeenCalled();
  });
});

function renderRow(onOpenTask: (taskId: string) => void) {
  let tree: ReturnType<typeof create> | undefined;
  act(() => {
    tree = create(
      <SidebarTaskRow
        onArchiveTask={vi.fn()}
        onOpenTask={onOpenTask}
        onRestoreTask={vi.fn()}
        showArchived={false}
        task={task()}
      />,
    );
  });
  return tree!;
}

function renderSidebar(onLoadNativeSessions: () => void) {
  const nativeSessions: NativeSessionsState = { items: [], loaded: true, loading: false };
  let tree: ReturnType<typeof create> | undefined;
  act(() => {
    tree = create(
      <Sidebar
        nativeSessionAgentId="codex"
        nativeSessionAgentName="Codex"
        nativeSessions={nativeSessions}
        onArchiveNativeSession={vi.fn()}
        onArchiveTask={vi.fn()}
        onLoadNativeSessions={onLoadNativeSessions}
        onNewTask={vi.fn()}
        onOpenNativeSession={vi.fn()}
        onOpenTask={vi.fn()}
        onRestoreNativeSession={vi.fn()}
        onRestoreTask={vi.fn()}
        onSearchChange={vi.fn()}
        onSettings={vi.fn()}
        onToggleArchived={vi.fn()}
        searchQuery=""
        showArchived={false}
        tasks={[task()]}
      />,
      // The list is scrolled to its top, where a pull may start.
      { createNodeMock: () => ({ scrollTop: 0, scrollHeight: 0, clientHeight: 0 }) },
    );
  });
  return tree!;
}

function menuTrigger(tree: ReturnType<typeof create>) {
  return tree.root.findByProps({ className: "task-row-action" });
}

function pointer(pointerType: string, clientX: number, clientY: number) {
  return { clientX, clientY, isPrimary: true, pointerId: 1, pointerType };
}

function touch(clientX: number, clientY: number) {
  return { touches: [{ clientX, clientY }] };
}

function task(): TaskSummary {
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
    pinned: false,
    updated_at: "2026-05-22T00:00:00.000Z",
    workspace_root: "/workspace",
  };
}
