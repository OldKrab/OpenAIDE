import { act, create, type ReactTestInstance } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ComposerAttachment } from "../state/composerOptions";
import { ComposerAttachments } from "./ComposerAttachments";

const TILE = { width: 76, height: 64, gap: 6 };

describe("Composer attachment reordering", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  it("moves an attachment one position with the arrow keys", () => {
    const onMoveAttachment = vi.fn();
    const { root } = render({ onMoveAttachment });

    keyDown(grip(root, "a.md"), "ArrowRight");
    expect(onMoveAttachment).toHaveBeenLastCalledWith("a", 1);
    expect(status(root)).toBe("a.md moved to position 2 of 3");

    keyDown(grip(root, "c.md"), "ArrowUp");
    expect(onMoveAttachment).toHaveBeenLastCalledWith("c", 1);

    onMoveAttachment.mockClear();
    keyDown(grip(root, "a.md"), "ArrowLeft");
    expect(onMoveAttachment).not.toHaveBeenCalled();
  });

  it("previews the new order while dragging and commits it on drop", () => {
    const onMoveAttachment = vi.fn();
    const onRemoveAttachment = vi.fn();
    const { root, list } = render({ onMoveAttachment, onRemoveAttachment });

    pointerDown(tile(root, "a.md"), { pointerType: "mouse", pointerId: 1, clientX: 10, clientY: 10 });
    pointerMove(list(), { pointerId: 1, clientX: slotCenter(2), clientY: 10 });

    expect(labels(root)).toEqual(["b.md", "c.md", "a.md"]);
    expect(onMoveAttachment).not.toHaveBeenCalled();

    act(() => list().props.onPointerUp({ pointerId: 1 }));
    expect(onMoveAttachment).toHaveBeenCalledWith("a", 2);

    // The release that ends a drag is not also a click on the tile's controls.
    const click = { preventDefault: vi.fn(), stopPropagation: vi.fn() };
    act(() => list().props.onClickCapture(click));
    expect(click.stopPropagation).toHaveBeenCalled();
  });

  it("drops where the dragged tile is shown when the pointer leaves the list", () => {
    const onMoveAttachment = vi.fn();
    const { root, list } = render({ onMoveAttachment });

    // Far right of and below the list: the tile stays clamped over the last slot.
    pointerDown(tile(root, "a.md"), { pointerType: "mouse", pointerId: 1, clientX: 10, clientY: 10 });
    pointerMove(list(), { pointerId: 1, clientX: 900, clientY: 400 });
    expect(labels(root)).toEqual(["b.md", "c.md", "a.md"]);

    act(() => list().props.onPointerUp({ pointerId: 1 }));
    expect(onMoveAttachment).toHaveBeenCalledWith("a", 2);
  });

  it("restores the order when the drag is cancelled", () => {
    const onMoveAttachment = vi.fn();
    const { root, list } = render({ onMoveAttachment });

    pointerDown(tile(root, "c.md"), { pointerType: "mouse", pointerId: 1, clientX: slotCenter(2), clientY: 10 });
    pointerMove(list(), { pointerId: 1, clientX: slotCenter(0), clientY: 10 });
    expect(labels(root)).toEqual(["c.md", "a.md", "b.md"]);

    act(() => list().props.onPointerCancel({ pointerId: 1 }));
    expect(labels(root)).toEqual(["a.md", "b.md", "c.md"]);
    expect(onMoveAttachment).not.toHaveBeenCalled();
  });

  it("leaves a touch on the tile body to list scrolling and reorders from the grip", () => {
    const onMoveAttachment = vi.fn();
    const { root, list } = render({ onMoveAttachment });

    pointerDown(tile(root, "a.md"), { pointerType: "touch", pointerId: 1, clientX: 10, clientY: 10 });
    pointerMove(list(), { pointerId: 1, clientX: slotCenter(1), clientY: 10 });
    act(() => list().props.onPointerUp({ pointerId: 1 }));
    expect(onMoveAttachment).not.toHaveBeenCalled();

    pointerDown(grip(root, "a.md"), { pointerType: "touch", pointerId: 2, clientX: 10, clientY: 10 });
    pointerMove(list(), { pointerId: 2, clientX: slotCenter(1), clientY: 10 });
    act(() => list().props.onPointerUp({ pointerId: 2 }));
    expect(onMoveAttachment).toHaveBeenCalledWith("a", 1);
  });

  it("offers no reorder control for a single, locked, or externally fixed list", () => {
    const grips = (root: ReactTestInstance) =>
      root.findAll((node) => typeof node.type === "string" && String(node.props["aria-label"]).startsWith("Reorder "));

    expect(grips(render({ onMoveAttachment: vi.fn(), attachments: [file("a")] }).root)).toHaveLength(0);
    expect(grips(render({ onMoveAttachment: vi.fn(), disabled: true }).root)).toHaveLength(0);
    expect(grips(render({}).root)).toHaveLength(0);
  });
});

function render(overrides: Partial<Parameters<typeof ComposerAttachments>[0]>) {
  let renderer: ReturnType<typeof create> | undefined;
  const element = (
    <ComposerAttachments
      agentLabel="Codex"
      attachments={[file("a"), file("b"), file("c")]}
      disabled={false}
      imageAttachmentsAllowed
      onRemoveAttachment={vi.fn()}
      {...overrides}
    />
  );
  act(() => {
    renderer = create(element, {
      // One row of equal tiles: enough layout for hit testing without a browser.
      createNodeMock: (node) => (node.props as { className?: string }).className === "composer-attachment-list"
        ? {
            getBoundingClientRect: () => ({ left: 0, top: 0 }),
            offsetHeight: TILE.height,
            offsetWidth: 3 * (TILE.width + TILE.gap),
            querySelectorAll: () => labels(renderer!.root).map((label, index) => ({
              dataset: { attachmentId: label.replace(".md", "") },
              getBoundingClientRect: () => ({ left: index * (TILE.width + TILE.gap), top: 0 }),
              offsetHeight: TILE.height,
              offsetLeft: index * (TILE.width + TILE.gap),
              offsetTop: 0,
              offsetWidth: TILE.width,
              querySelector: () => null,
              style: {},
            })),
            setPointerCapture: vi.fn(),
          }
        : null,
    });
  });
  const root = renderer!.root;
  return { root, list: () => root.findByProps({ className: "composer-attachment-list" }) };
}

function file(id: string): ComposerAttachment {
  return { kind: "file", label: `${id}.md`, local_id: id, app_server_handle_id: `handle-${id}` as never };
}

function slotCenter(index: number) {
  return index * (TILE.width + TILE.gap) + TILE.width / 2;
}

function labels(root: ReactTestInstance) {
  return root.findAllByProps({ className: "composer-file-attachment-label" }).map((node) => node.children.join(""));
}

function tile(root: ReactTestInstance, label: string) {
  return root.findAll((node) => typeof node.type === "string" && node.props.title === label
    && node.props["data-attachment-id"] !== undefined)[0]!;
}

function grip(root: ReactTestInstance, label: string) {
  return root.findByProps({ "aria-label": `Reorder ${label}` });
}

function status(root: ReactTestInstance) {
  return root.findByProps({ "aria-live": "polite" }).children.join("");
}

function keyDown(instance: ReactTestInstance, key: string) {
  act(() => instance.props.onKeyDown({ key, preventDefault: vi.fn() }));
}

function pointerDown(instance: ReactTestInstance, event: Record<string, unknown>) {
  act(() => instance.props.onPointerDown({ button: 0, stopPropagation: vi.fn(), ...event }));
}

function pointerMove(instance: ReactTestInstance, event: Record<string, unknown>) {
  act(() => instance.props.onPointerMove({ preventDefault: vi.fn(), ...event }));
}
