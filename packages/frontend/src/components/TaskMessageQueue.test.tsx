// @vitest-environment jsdom

import { act, create } from "react-test-renderer";
import { describe, expect, it, vi } from "vitest";
import { TaskMessageQueueView } from "./TaskMessageQueue";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("Task Message Queue", () => {
  it("requests immediate removal without adding confirmation UI", () => {
    const onRemove = vi.fn();
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        queue={{
          revision: 3,
          items: [{
            queued_message_id: "queued-1",
            text: "Do this after the current turn",
            created_at: "2026-08-03T00:00:00Z",
          }],
        }}
        onRemove={onRemove}
      />);
    });

    act(() => tree.root.findByProps({ "aria-label": "Delete queued message" }).props.onClick());

    expect(onRemove).toHaveBeenCalledWith("queued-1");
    expect(JSON.stringify(tree.toJSON())).not.toContain("Undo");
    expect(JSON.stringify(tree.toJSON())).not.toContain("Confirm");
  });

  it("collapses the rows behind the bottom title", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        queue={{
          revision: 1,
          items: [
            { queued_message_id: "queued-1", text: "Next", created_at: "now" },
            { queued_message_id: "queued-2", text: "Later", created_at: "now" },
          ],
        }}
        onRemove={vi.fn()}
      />);
    });

    act(() => tree.root.findByProps({ "aria-label": "Collapse message queue" }).props.onClick());

    expect(tree.root.findByProps({ className: "task-message-queue-disclosure" }).props).toMatchObject({
      "aria-hidden": true,
      "data-open": false,
      inert: true,
    });
  });

  it("reports expansion so Chat can leave follow mode before Queue changes size", () => {
    const onExpanded = vi.fn();
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        queue={{
          revision: 1,
          items: [
            { queued_message_id: "queued-1", text: "Next", created_at: "now" },
            { queued_message_id: "queued-2", text: "Later", created_at: "now" },
          ],
        }}
        onExpanded={onExpanded}
        onRemove={vi.fn()}
      />);
    });
    expect(onExpanded).toHaveBeenCalledOnce();

    act(() => tree.root.findByProps({ "aria-label": "Collapse message queue" }).props.onClick());
    act(() => tree.root.findByProps({ "aria-label": "Expand message queue" }).props.onClick());

    expect(onExpanded).toHaveBeenCalledTimes(2);
  });

  it("renders one queued message without disclosure or reorder controls", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        queue={{
          revision: 1,
          items: [{ queued_message_id: "queued-1", text: "Next", created_at: "now" }],
        }}
        onMove={vi.fn()}
        onRemove={vi.fn()}
      />);
    });

    expect(tree.root.findByProps({ className: "task-message-queue" }).props["data-single"]).toBe(true);
    expect(tree.root.findAllByProps({ "aria-label": "Collapse message queue" })).toHaveLength(0);
    expect(tree.root.findAllByProps({ "aria-label": "Expand message queue" })).toHaveLength(0);
    expect(tree.root.findAllByProps({ "aria-label": "Reorder Next" })).toHaveLength(0);
    expect(tree.root.findByProps({ className: "task-message-queue-disclosure" }).props).toMatchObject({
      "aria-hidden": false,
      "data-open": true,
      inert: undefined,
    });
  });

  it("returns a queued message to the composer for editing", () => {
    const onTake = vi.fn();
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        queue={{
          revision: 1,
          items: [{ queued_message_id: "queued-1", text: "Original", created_at: "now" }],
        }}
        onMove={vi.fn()}
        onRemove={vi.fn()}
        onTake={onTake}
      />);
    });

    act(() => tree.root.findByProps({ "aria-label": "Edit in Composer" }).props.onClick());

    expect(onTake).toHaveBeenCalledWith("queued-1");
    expect(tree.root.findAllByProps({ "aria-label": "Queued message text" })).toHaveLength(0);
  });

  it("disables Edit in Composer when the composer already has a draft", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        editDisabled
        queue={{
          revision: 1,
          items: [{ queued_message_id: "queued-1", text: "Original", created_at: "now" }],
        }}
        onTake={vi.fn()}
        onRemove={vi.fn()}
      />);
    });

    expect(tree.root.findByProps({ "aria-label": "Edit in Composer" }).props.disabled).toBe(true);
  });

  it("keeps a dequeuing row visible and pending until extraction succeeds", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        extractingQueuedMessageId="queued-1"
        queue={{ revision: 2, items: [
          { queued_message_id: "queued-1", text: "Original", created_at: "now" },
          { queued_message_id: "queued-2", text: "Later", created_at: "now" },
        ] }}
        onTake={vi.fn()}
        onMove={vi.fn()}
        onRemove={vi.fn()}
        onSendNow={vi.fn()}
      />);
    });

    const pendingRow = tree.root.findByProps({ "data-queue-id": "queued-1" });
    const laterRow = tree.root.findByProps({ "data-queue-id": "queued-2" });
    expect(pendingRow.props["data-extracting"]).toBe("pending");
    expect(tree.root.findAllByProps({ "data-queue-row": true })).toHaveLength(2);
    expect(pendingRow.findByProps({ "aria-label": "Moving queued message to Composer" }).props.disabled).toBe(true);
    expect(laterRow.findByProps({ "aria-label": "Edit in Composer" }).props.disabled).toBe(true);
    expect(tree.root.findByProps({ "aria-label": "Reorder Original" }).props.disabled).toBe(true);
  });

  it("moves a keyboard-accessible queued row", () => {
    const onMove = vi.fn();
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        queue={{
          revision: 1,
          items: [
            { queued_message_id: "queued-1", text: "First", created_at: "now" },
            { queued_message_id: "queued-2", text: "Second", created_at: "now" },
          ],
        }}
        onMove={onMove}
        onRemove={vi.fn()}
      />);
    });

    act(() => tree.root.findByProps({ "aria-label": "Reorder First" }).props.onKeyDown({
      key: "ArrowDown",
      preventDefault: vi.fn(),
    }));

    expect(onMove).toHaveBeenCalledWith("queued-1", 1);
  });

  it("sends a selected queued row now", () => {
    const onSendNow = vi.fn();
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        queue={{ revision: 4, items: [
          { queued_message_id: "queued-1", text: "Send this", created_at: "now" },
        ] }}
        onRemove={vi.fn()}
        onSendNow={onSendNow}
      />);
    });

    act(() => tree.root.findByProps({ "aria-label": "Send queued message now" }).props.onClick());

    expect(onSendNow).toHaveBeenCalledWith("queued-1");
  });

  it("blocks every revision-sensitive action while a queue mutation is pending", async () => {
    let resolveRemove!: () => void;
    const onRemove = vi.fn(() => new Promise<void>((resolve) => { resolveRemove = resolve; }));
    const onSendNow = vi.fn(async () => undefined);
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        queue={{ revision: 4, items: [
          { queued_message_id: "queued-1", text: "First", created_at: "now" },
          { queued_message_id: "queued-2", text: "Second", created_at: "now" },
        ] }}
        onMove={vi.fn(async () => undefined)}
        onRemove={onRemove}
        onSendNow={onSendNow}
        onTake={vi.fn()}
      />);
    });

    act(() => tree.root.findAllByProps({ "aria-label": "Delete queued message" })[0].props.onClick());

    expect(tree.root.findAllByProps({ "aria-label": "Delete queued message" }).every((button) => button.props.disabled)).toBe(true);
    expect(tree.root.findAllByProps({ "aria-label": "Send queued message now" }).every((button) => button.props.disabled)).toBe(true);
    expect(tree.root.findAllByProps({ "aria-label": "Edit in Composer" }).every((button) => button.props.disabled)).toBe(true);
    expect(tree.root.findAll((node) => typeof node.props["aria-label"] === "string"
      && node.props["aria-label"].startsWith("Reorder ")).every((button) => button.props.disabled)).toBe(true);
    act(() => tree.root.findAllByProps({ "aria-label": "Delete queued message" })[1].props.onClick());
    expect(onRemove).toHaveBeenCalledOnce();

    await act(async () => resolveRemove());
    expect(tree.root.findAllByProps({ "aria-label": "Delete queued message" }).every((button) => !button.props.disabled)).toBe(true);
  });

  it("keeps a touch row swipe as scrolling unless it starts on the grip", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        queue={{ revision: 1, items: [
          { queued_message_id: "queued-1", text: "First", created_at: "now" },
          { queued_message_id: "queued-2", text: "Second", created_at: "now" },
        ] }}
        onMove={vi.fn()}
        onRemove={vi.fn()}
      />);
    });
    const row = tree.root.findByProps({ "data-queue-id": "queued-1" });
    const list = tree.root.findByProps({ className: "task-message-queue-items" });

    act(() => row.props.onPointerDown({
      pointerType: "touch", pointerId: 1, clientY: 10,
      currentTarget: { setPointerCapture: vi.fn() },
    }));
    act(() => list.props.onPointerMove({
      pointerId: 1, clientY: 40, preventDefault: vi.fn(),
      currentTarget: { querySelectorAll: () => [] },
    }));

    expect(row.props["data-dragging"]).toBeUndefined();
  });

  it("prevents native text dragging when mouse reordering starts", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        queue={{ revision: 1, items: [
          { queued_message_id: "queued-1", text: "First", created_at: "now" },
          { queued_message_id: "queued-2", text: "Second", created_at: "now" },
        ] }}
        onMove={vi.fn()}
        onRemove={vi.fn()}
      />);
    });
    const preventDefault = vi.fn();

    act(() => tree.root.findByProps({ "data-queue-id": "queued-1" }).props.onPointerDown({
      pointerType: "mouse",
      pointerId: 2,
      clientY: 10,
      preventDefault,
      currentTarget: { setPointerCapture: vi.fn() },
    }));

    expect(preventDefault).toHaveBeenCalledOnce();
  });

  it("places the insertion marker on a destination row, never the dragged row", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        queue={{ revision: 1, items: [
          { queued_message_id: "queued-1", text: "First", created_at: "now" },
          { queued_message_id: "queued-2", text: "Second", created_at: "now" },
        ] }}
        onMove={vi.fn()}
        onRemove={vi.fn()}
      />);
    });
    const list = tree.root.findByProps({ className: "task-message-queue-items" });
    act(() => tree.root.findByProps({ "data-queue-id": "queued-1" }).props.onPointerDown({
      pointerType: "mouse", pointerId: 2, clientY: 10,
      preventDefault: vi.fn(),
      currentTarget: { setPointerCapture: vi.fn() },
    }));
    act(() => list.props.onPointerMove({
      pointerId: 2, clientY: 50, preventDefault: vi.fn(),
      currentTarget: { querySelectorAll: () => [
        { dataset: { queueId: "queued-1" }, getBoundingClientRect: () => ({ top: 0 }), offsetHeight: 30 },
        { dataset: { queueId: "queued-2" }, getBoundingClientRect: () => ({ top: 40 }), offsetHeight: 30 },
      ] },
    }));

    expect(tree.root.findByProps({ "data-queue-id": "queued-1" }).props["data-drop-before"]).toBeUndefined();
    expect(tree.root.findByProps({ "data-queue-id": "queued-2" }).props["data-drop-after"]).toBe(true);
    expect(tree.root.findByProps({ className: "task-message-queue-drop-marker" }).props["data-position"])
      .toBe("after");
  });

  it("uses the dragged row center instead of the pointer grab position", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        queue={{ revision: 1, items: [
          { queued_message_id: "queued-1", text: "First", created_at: "now" },
          { queued_message_id: "queued-2", text: "Second", created_at: "now" },
        ] }}
        onMove={vi.fn()}
        onRemove={vi.fn()}
      />);
    });
    const list = tree.root.findByProps({ className: "task-message-queue-items" });
    act(() => tree.root.findByProps({ "data-queue-id": "queued-1" }).props.onPointerDown({
      pointerType: "mouse", pointerId: 2, clientY: 2,
      preventDefault: vi.fn(),
      currentTarget: { setPointerCapture: vi.fn() },
    }));
    act(() => list.props.onPointerMove({
      pointerId: 2, clientY: 47, preventDefault: vi.fn(),
      currentTarget: { querySelectorAll: () => [
        { dataset: { queueId: "queued-1" }, getBoundingClientRect: () => ({ top: 0 }), offsetHeight: 30 },
        { dataset: { queueId: "queued-2" }, getBoundingClientRect: () => ({ top: 40 }), offsetHeight: 30 },
      ] },
    }));

    expect(tree.root.findByProps({ "data-queue-id": "queued-2" }).props["data-drop-after"])
      .toBe(true);
  });

  it("shows the dropped order before the durable move settles", () => {
    const onMove = vi.fn(() => new Promise<void>(() => undefined));
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        queue={{ revision: 1, items: [
          { queued_message_id: "queued-1", text: "First", created_at: "now" },
          { queued_message_id: "queued-2", text: "Second", created_at: "now" },
        ] }}
        onMove={onMove}
        onRemove={vi.fn()}
      />);
    });
    const list = tree.root.findByProps({ className: "task-message-queue-items" });
    act(() => tree.root.findByProps({ "data-queue-id": "queued-1" }).props.onPointerDown({
      pointerType: "mouse", pointerId: 2, clientY: 10,
      preventDefault: vi.fn(),
      currentTarget: { setPointerCapture: vi.fn() },
    }));
    act(() => list.props.onPointerMove({
      pointerId: 2, clientY: 90, preventDefault: vi.fn(),
      currentTarget: { querySelectorAll: () => [
        { dataset: { queueId: "queued-1" }, getBoundingClientRect: () => ({ top: 0 }), offsetHeight: 30 },
        { dataset: { queueId: "queued-2" }, getBoundingClientRect: () => ({ top: 40 }), offsetHeight: 30 },
      ] },
    }));
    act(() => list.props.onPointerUp({ pointerId: 2 }));

    expect(onMove).toHaveBeenCalledWith("queued-1", 1);
    expect(tree.root.findAllByProps({ "data-queue-row": true }).map((row) => row.props["data-queue-id"]))
      .toEqual(["queued-2", "queued-1"]);

    act(() => tree.update(<TaskMessageQueueView
      queue={{ revision: 2, items: [
        { queued_message_id: "queued-2", text: "Second", created_at: "now" },
        { queued_message_id: "queued-1", text: "First", created_at: "now" },
      ] }}
      onMove={onMove}
      onRemove={vi.fn()}
    />));
    act(() => tree.update(<TaskMessageQueueView
      queue={{ revision: 3, items: [
        { queued_message_id: "queued-1", text: "First", created_at: "now" },
        { queued_message_id: "queued-2", text: "Second", created_at: "now" },
      ] }}
      onMove={onMove}
      onRemove={vi.fn()}
    />));

    expect(tree.root.findAllByProps({ "data-queue-row": true }).map((row) => row.props["data-queue-id"]))
      .toEqual(["queued-1", "queued-2"]);
  });

  it("restores the durable order when a dropped move fails", async () => {
    let rejectMove!: (error: Error) => void;
    const onMove = vi.fn(() => new Promise<void>((_resolve, reject) => { rejectMove = reject; }));
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<TaskMessageQueueView
        queue={{ revision: 1, items: [
          { queued_message_id: "queued-1", text: "First", created_at: "now" },
          { queued_message_id: "queued-2", text: "Second", created_at: "now" },
        ] }}
        onMove={onMove}
        onRemove={vi.fn()}
      />);
    });
    const list = tree.root.findByProps({ className: "task-message-queue-items" });
    act(() => tree.root.findByProps({ "data-queue-id": "queued-1" }).props.onPointerDown({
      pointerType: "mouse", pointerId: 2, clientY: 10,
      preventDefault: vi.fn(), currentTarget: { setPointerCapture: vi.fn() },
    }));
    act(() => list.props.onPointerMove({
      pointerId: 2, clientY: 90, preventDefault: vi.fn(),
      currentTarget: { querySelectorAll: () => [
        { dataset: { queueId: "queued-1" }, getBoundingClientRect: () => ({ top: 0 }), offsetHeight: 30 },
        { dataset: { queueId: "queued-2" }, getBoundingClientRect: () => ({ top: 40 }), offsetHeight: 30 },
      ] },
    }));
    act(() => list.props.onPointerUp({ pointerId: 2 }));

    await act(async () => rejectMove(new Error("move failed")));

    expect(tree.root.findAllByProps({ "data-queue-row": true }).map((row) => row.props["data-queue-id"]))
      .toEqual(["queued-1", "queued-2"]);
  });
});
