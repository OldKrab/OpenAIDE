import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatMessage } from "@openaide/app-shell-contracts";
import { createChatRevealStore } from "../state/chatReveal";
import type { TaskChatTimelineRow } from "./TaskChatTimeline";
import { useChatMessageReveal } from "./useChatMessageReveal";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

function messageRow(id: string): TaskChatTimelineRow {
  return { key: `message:${id}`, kind: "message", message: { message_id: id } as ChatMessage };
}

type Scenario = {
  rows: TaskChatTimelineRow[];
  hasEarlier?: boolean;
  pending?: boolean;
  taskId?: string;
};

function mount(initial: Scenario) {
  const store = createChatRevealStore();
  const scrollToIndex = vi.fn();
  const loadEarlier = vi.fn(() => 1);
  const pauseFollowing = vi.fn();
  const highlighted: Array<string | undefined> = [];
  function Harness({ rows, hasEarlier = false, pending = false, taskId = "task_1" }: Scenario) {
    highlighted.push(useChatMessageReveal({
      beforeCursor: hasEarlier ? "cursor_1" : undefined,
      hasEarlier,
      loadEarlier,
      pauseFollowing,
      pending,
      rows,
      scrollToIndex,
      store,
      taskId,
    }));
    return null;
  }
  let tree!: ReturnType<typeof create>;
  act(() => { tree = create(<Harness {...initial} />); });
  return {
    store, scrollToIndex, loadEarlier, pauseFollowing, highlighted,
    update: (next: Scenario) => act(() => tree.update(<Harness {...next} />)),
  };
}

describe("useChatMessageReveal", () => {
  it("centers a loaded message, pauses Follow and marks it", () => {
    const view = mount({ rows: [messageRow("a"), messageRow("b")] });

    act(() => view.store.request("task_1", "b"));

    expect(view.pauseFollowing).toHaveBeenCalled();
    expect(view.scrollToIndex).toHaveBeenCalledWith(1);
    expect(view.highlighted.at(-1)).toBe("b");
    expect(view.store.getSnapshot().request).toBeUndefined();
  });

  it("loads earlier pages until the message arrives, then scrolls to it", () => {
    const view = mount({ rows: [messageRow("c")], hasEarlier: true });

    act(() => view.store.request("task_1", "a"));
    expect(view.loadEarlier).toHaveBeenCalledWith("cursor_1");
    expect(view.scrollToIndex).not.toHaveBeenCalled();

    view.update({ rows: [messageRow("c")], hasEarlier: true, pending: true });
    expect(view.loadEarlier).toHaveBeenCalledTimes(1);

    view.update({ rows: [messageRow("a"), messageRow("c")], hasEarlier: false });
    expect(view.scrollToIndex).toHaveBeenCalledWith(0);
  });

  it("gives up when the whole Chat is loaded and the message is not in it", () => {
    const view = mount({ rows: [messageRow("a")] });

    act(() => view.store.request("task_1", "missing"));

    expect(view.scrollToIndex).not.toHaveBeenCalled();
    expect(view.store.getSnapshot().request).toBeUndefined();
  });

  it("waits while the Task's Chat has not arrived", () => {
    const view = mount({ rows: [] });

    act(() => view.store.request("task_1", "a"));
    expect(view.store.getSnapshot().request).toBeDefined();

    view.update({ rows: [messageRow("a")] });
    expect(view.scrollToIndex).toHaveBeenCalledWith(0);
  });

  it("leaves a request for another Task alone", () => {
    const view = mount({ rows: [messageRow("a")], taskId: "task_2" });

    act(() => view.store.request("task_1", "a"));

    expect(view.scrollToIndex).not.toHaveBeenCalled();
    expect(view.store.getSnapshot().request).toBeDefined();
  });
});
