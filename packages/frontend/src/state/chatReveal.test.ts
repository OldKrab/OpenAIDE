import { describe, expect, it, vi } from "vitest";
import { createChatRevealStore, REVEAL_HIGHLIGHT_MS, REVEAL_REQUEST_TTL_MS } from "./chatReveal";

function store() {
  let now = 1_000; // timing: mocked
  const scheduled: Array<{ callback: () => void; delayMs: number }> = [];
  const reveal = createChatRevealStore(() => now, (callback, delayMs) => { scheduled.push({ callback, delayMs }); });
  return { reveal, scheduled, advance: (ms: number) => { now += ms; } };
}

describe("chatReveal", () => {
  it("offers a request only to the Task it names", () => {
    const { reveal } = store();
    reveal.request("task_1", "message_1");

    expect(reveal.pendingFor("task_2")).toBeUndefined();
    expect(reveal.pendingFor("task_1")?.messageId).toBe("message_1");
  });

  it("drops a request that outlived its window", () => {
    const { reveal, advance } = store();
    const listener = vi.fn();
    reveal.subscribe(listener);
    reveal.request("task_1", "message_1");
    advance(REVEAL_REQUEST_TTL_MS + 1);

    expect(reveal.pendingFor("task_1")).toBeUndefined();
    expect(reveal.getSnapshot().request).toBeUndefined();
  });

  it("marks the revealed message and clears the mark after its window", () => {
    const { reveal, scheduled } = store();
    reveal.request("task_1", "message_1");
    const request = reveal.pendingFor("task_1")!;

    reveal.settle(request, true);

    expect(reveal.getSnapshot()).toEqual({ highlighted: { taskId: "task_1", messageId: "message_1" } });
    expect(scheduled).toHaveLength(1);
    expect(scheduled[0]!.delayMs).toBe(REVEAL_HIGHLIGHT_MS);
    scheduled[0]!.callback();
    expect(reveal.getSnapshot().highlighted).toBeUndefined();
  });

  it("ends a request whose message is not in the Chat without marking anything", () => {
    const { reveal } = store();
    reveal.request("task_1", "message_1");

    reveal.settle(reveal.pendingFor("task_1")!, false);

    expect(reveal.getSnapshot()).toEqual({ request: undefined });
  });

  it("ignores the settlement of a request a newer one replaced", () => {
    const { reveal } = store();
    reveal.request("task_1", "old");
    const old = reveal.pendingFor("task_1")!;
    reveal.request("task_1", "new");

    reveal.settle(old, true);

    expect(reveal.pendingFor("task_1")?.messageId).toBe("new");
    expect(reveal.getSnapshot().highlighted).toBeUndefined();
  });
});
