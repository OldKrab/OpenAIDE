import { act, create, type ReactTestRenderer } from "react-test-renderer";
import { beforeEach, expect, it, vi } from "vitest";
import type { DeleteSessionAction } from "../intents/sessionDeletionIntent";
import { useSessionLifecycleDialog } from "./SessionLifecycleDialog";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});
function Harness({ onDelete }: { onDelete: DeleteSessionAction }) {
  const lifecycle = useSessionLifecycleDialog({ title: "Session A", target: { kind: "task", taskId: "task-a" as never }, onArchive: vi.fn(), onDelete });
  return <><button onClick={lifecycle.delete}>Delete</button>{lifecycle.dialog}</>;
}
function button(tree: ReactTestRenderer, text: string) {
  return tree.root.findAllByType("button").find((item) => item.children.includes(text))!;
}
it("requires ordinary and active-work confirmations and sends the authoritative queued count", async () => {
  const onDelete = vi.fn<DeleteSessionAction>().mockResolvedValueOnce({ kind: "confirmationRequired", title: "Session A", active: true, queuedMessageCount: 2 })
    .mockResolvedValueOnce({ kind: "deleted", reference: { agentId: "codex" as never, sessionId: "native-a" }, projectId: "project-a" as never, taskId: "task-a" as never });
  let tree!: ReactTestRenderer;
  act(() => { tree = create(<Harness onDelete={onDelete} />); });
  await act(async () => button(tree, "Delete").props.onClick());
  expect(onDelete).toHaveBeenCalledTimes(1);
  expect(tree.root.findAllByType("p").some((p) => p.children.includes("2 queued messages will be discarded."))).toBe(true);
  await act(async () => button(tree, "Continue").props.onClick());
  expect(onDelete).toHaveBeenCalledTimes(1);
  expect(tree.root.findByProps({ role: "dialog" }).props["aria-label"]).toBe("Delete while work is active?");
  await act(async () => button(tree, "Delete session").props.onClick());
  expect(onDelete).toHaveBeenLastCalledWith({ target: { kind: "task", taskId: "task-a" }, confirmation: { active: true, queuedMessageCount: 2 } });
  expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(0);
  act(() => tree.unmount());
});

it("keeps a failed deletion visible and retries only after another user action", async () => {
  const onDelete = vi.fn<DeleteSessionAction>().mockResolvedValueOnce({ kind: "confirmationRequired", title: "Session A", active: false, queuedMessageCount: 0 })
    .mockRejectedValueOnce(new Error("Deletion outcome is unknown."))
    .mockResolvedValueOnce({ kind: "confirmationRequired", title: "Session A", active: true, queuedMessageCount: 1 });
  let tree!: ReactTestRenderer;
  act(() => { tree = create(<Harness onDelete={onDelete} />); });
  await act(async () => button(tree, "Delete").props.onClick());
  await act(async () => button(tree, "Delete session").props.onClick());
  expect(tree.root.findByProps({ role: "alert" }).children).toContain("Deletion outcome is unknown.");
  expect(onDelete).toHaveBeenCalledTimes(2);
  await act(async () => button(tree, "Retry Delete").props.onClick());
  expect(onDelete).toHaveBeenCalledTimes(3);
  expect(button(tree, "Continue")).toBeDefined();
  act(() => tree.unmount());
});
