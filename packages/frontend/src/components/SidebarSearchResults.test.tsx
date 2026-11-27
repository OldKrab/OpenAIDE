import { act, create } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskNavigationSearchHit, TaskNavigationSearchResult } from "@openaide/app-server-client";
import { chatReveal } from "../state/chatReveal";
import { Highlighted, SidebarSearchResults } from "./SidebarSearchResults";
import { NAVIGATION_SEARCH_DEBOUNCE_MS } from "./useNavigationSearch";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  vi.useFakeTimers();
});

afterEach(() => vi.useRealTimers());

function taskHit(overrides: Partial<TaskNavigationSearchHit> & { taskId?: string; title?: string } = {}): TaskNavigationSearchHit {
  const { taskId = "task_1", title = "Parser work", ...rest } = overrides;
  return {
    row: {
      live: false,
      entry: {
        kind: "task",
        task: {
          taskId,
          agentId: "codex",
          projectId: "project_1",
          title: { value: title, source: "user" },
          status: "inactive",
          hasMessages: true,
          unread: false,
          updatedAt: "2026-05-22T00:00:00.000Z",
          lastActivity: "2026-05-22T00:00:00.000Z",
        } as never,
      },
    },
    section: "tasks",
    ...rest,
  };
}

function sessionHit(): TaskNavigationSearchHit {
  return {
    row: {
      live: false,
      entry: {
        kind: "nativeSession",
        session: {
          reference: { agentId: "codex", sessionId: "native_1" },
          projectId: "project_1",
          workspaceRoot: "/workspace",
          title: "Old native chat",
          pinned: false,
          lastActivity: null,
        } as never,
      },
    },
    section: "archive",
    titleHighlights: [[4, 10]],
  };
}

async function mount(result: TaskNavigationSearchResult | Error, props: Partial<Parameters<typeof SidebarSearchResults>[0]> = {}) {
  const search = vi.fn(async () => {
    if (result instanceof Error) throw result;
    return result;
  });
  const onOpenTask = vi.fn();
  const onOpenNativeSession = vi.fn();
  let tree!: ReturnType<typeof create>;
  act(() => {
    tree = create(
      <SidebarSearchResults
        onOpenNativeSession={onOpenNativeSession}
        onOpenTask={onOpenTask}
        query="pars"
        search={search}
        {...props}
      />,
    );
  });
  await act(async () => { await vi.advanceTimersByTimeAsync(NAVIGATION_SEARCH_DEBOUNCE_MS); });
  return { tree, search, onOpenTask, onOpenNativeSession };
}

const marks = (tree: ReturnType<typeof create>) =>
  tree.root.findAllByType("mark").map((node) => node.children.join(""));

describe("SidebarSearchResults", () => {
  it("shows the title and snippet with their highlighted ranges", async () => {
    const { tree } = await mount({
      hits: [taskHit({
        titleHighlights: [[0, 6]],
        snippet: { text: "the parser fails", highlights: [[4, 10]] },
        messageId: "message_9" as never,
      })],
      truncated: false,
    });

    expect(marks(tree)).toEqual(["Parser", "parser"]);
  });

  it("opens a Task hit at its message", async () => {
    const request = vi.spyOn(chatReveal, "request");
    const { tree, onOpenTask } = await mount({
      hits: [taskHit({ snippet: { text: "x", highlights: [] }, messageId: "message_9" as never })],
      truncated: false,
    });

    act(() => tree.root.findByType("button").props.onClick());

    expect(request).toHaveBeenCalledWith("task_1", "message_9");
    expect(onOpenTask).toHaveBeenCalledWith("task_1");
    request.mockRestore();
  });

  it("opens a title-only hit without asking the Chat to scroll", async () => {
    const request = vi.spyOn(chatReveal, "request");
    const { tree, onOpenTask } = await mount({ hits: [taskHit()], truncated: false });

    act(() => tree.root.findByType("button").props.onClick());

    expect(request).not.toHaveBeenCalled();
    expect(onOpenTask).toHaveBeenCalledWith("task_1");
    request.mockRestore();
  });

  it("opens a Native Session hit and marks archived rows", async () => {
    const { tree, onOpenNativeSession } = await mount({ hits: [sessionHit()], truncated: false });

    expect(tree.root.findAllByProps({ "aria-label": "Archived" }).length).toBeGreaterThan(0);
    expect(marks(tree)).toEqual(["native"]);
    act(() => tree.root.findByType("button").props.onClick());

    expect(onOpenNativeSession).toHaveBeenCalledWith(expect.objectContaining({
      agent_id: "codex",
      session_id: "native_1",
      project_id: "project_1",
      title: "Old native chat",
    }));
  });

  it("says when nothing matched, when the answer was cut, and when the search failed", async () => {
    const empty = await mount({ hits: [], truncated: false });
    expect(JSON.stringify(empty.tree.toJSON())).toContain("No results");

    const cut = await mount({ hits: [taskHit()], truncated: true });
    expect(JSON.stringify(cut.tree.toJSON())).toContain("Showing the best matches");

    const failed = await mount(new Error("offline"));
    expect(JSON.stringify(failed.tree.toJSON())).toContain("Search failed");
  });
});

describe("Highlighted", () => {
  it("slices by UTF-16 units and skips ranges outside the text", () => {
    let tree!: ReturnType<typeof create>;
    act(() => { tree = create(<Highlighted ranges={[[3, 5], [2, 4], [9, 20]]} text={"\u{1F600} ab cd"} />); });

    expect(tree.root.findAllByType("mark").map((node) => node.children.join(""))).toEqual(["ab"]);
  });
});
