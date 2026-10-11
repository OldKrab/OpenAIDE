import { act, create } from "react-test-renderer";
import { beforeEach, expect, it, vi } from "vitest";
import type { RemoteComputer, RemoteComputersCapability } from "../../services/frontendShell";
import { RemoteComputersSettings } from "./RemoteComputersSettings";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const studio = { id: "a".repeat(64), name: "Studio", current: false };
const laptop = { id: "b".repeat(64), name: "Laptop", current: false };

function capability(computers: RemoteComputer[] = []) {
  const value = {
    list: vi.fn().mockResolvedValue(computers),
    pair: vi.fn().mockResolvedValue(undefined),
    use: vi.fn().mockResolvedValue(undefined),
    forget: vi.fn().mockResolvedValue(undefined),
  } satisfies RemoteComputersCapability;
  return value;
}
async function render(value: RemoteComputersCapability) {
  let tree!: ReturnType<typeof create>;
  await act(async () => { tree = create(<RemoteComputersSettings capability={value} />); });
  return tree;
}
const press = (tree: ReturnType<typeof create>, label: string, index = 0) =>
  act(async () => tree.root.findAllByType("button").filter((button) => button.children.includes(label))[index]!.props.onClick());

it("marks this computer in use until a paired computer is chosen", async () => {
  const test = capability([studio, laptop]);
  const tree = await render(test);
  expect(tree.root.findAllByProps({ className: "connection-status" })).toHaveLength(1);
  // This computer is in use, so only the two paired computers offer Use.
  await press(tree, "Use", 1);
  expect(test.use).toHaveBeenLastCalledWith(laptop.id);
  act(() => tree.unmount());
});

it("returns to this computer from a paired one", async () => {
  const test = capability([{ ...studio, current: true }]);
  const tree = await render(test);
  await press(tree, "Use");
  expect(test.use).toHaveBeenLastCalledWith(undefined);
  act(() => tree.unmount());
});

it("forgets a computer only after the user confirms and says where it stays listed", async () => {
  const test = capability([{ ...studio, current: true }]);
  const tree = await render(test);
  await press(tree, "Forget");
  expect(test.forget).not.toHaveBeenCalled();
  const confirmation = tree.root.findByProps({ role: "alertdialog" }).findByType("p").children.join("");
  expect(confirmation).toContain("works on this computer");
  expect(confirmation).toContain("until you remove it there");
  await press(tree, "Forget", 1);
  expect(test.forget).toHaveBeenLastCalledWith(studio.id);
  act(() => tree.unmount());
});

it("pairs with a pasted code and shows the shell's reason when it is refused", async () => {
  const test = capability();
  test.pair.mockRejectedValue("This code has expired or was already used. Show a new code on the other computer.");
  const tree = await render(test);
  await press(tree, "Pair");
  const submit = () => tree.root.findByType("form").props.onSubmit({ preventDefault() {} });
  expect(tree.root.findByProps({ type: "submit" }).props.disabled).toBe(true);
  await act(async () => tree.root.findByType("textarea").props.onChange({ currentTarget: { value: "OAI1 ABCD" } }));
  await act(async () => submit());
  expect(test.pair).toHaveBeenLastCalledWith("OAI1 ABCD");
  expect(tree.root.findByProps({ role: "alert" }).children.join("")).toContain("already used");
  expect(JSON.stringify(tree.toJSON())).toContain("public relays run by the iroh project");
  act(() => tree.unmount());
});
