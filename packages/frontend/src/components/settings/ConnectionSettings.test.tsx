import { act, create } from "react-test-renderer";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import * as shell from "../../services/frontendShell";
import type { ConnectionSettings, ConnectionSnapshot } from "../../services/connectionSettings";
import { ConnectionSettingsTab } from "./ConnectionSettingsTab";
import { GeneralSettingsTab } from "./GeneralSettingsTab";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});
afterEach(() => vi.restoreAllMocks());
function capability() {
  let state: ConnectionSnapshot = {
    remote: false, paired: false, computer: "", computers: [], busy: false, notice: "", termux: true, permission: true,
    background: true, appBattery: true, termuxBattery: true, batterySaver: false, notifications: true, checks: null,
  };
  let listener = () => {};
  const execute = vi.fn().mockResolvedValue(undefined);
  const value: ConnectionSettings = {
    snapshot: () => state, subscribe: changed => { listener = changed; return () => {}; }, execute,
  };
  return { value, execute, update: (changes: Partial<ConnectionSnapshot>) => { state = { ...state, ...changes }; listener(); } };
}
async function render(test: ReturnType<typeof capability>) {
  let tree!: ReturnType<typeof create>;
  await act(async () => { tree = create(<ConnectionSettingsTab capability={test.value} />); });
  return tree;
}
it("removes the separate Manage screen from General for every shell", () => {
  vi.spyOn(shell, "currentFrontendShell").mockReturnValue({ connectionSettings: capability().value } as unknown as shell.FrontendShell);
  let tree!: ReturnType<typeof create>;
  act(() => { tree = create(<GeneralSettingsTab preferences={{ composer_submit_shortcut: "enter" }} onSetComposerSubmitShortcut={() => {}} />); });
  expect(JSON.stringify(tree.toJSON())).not.toContain("Manage");
  act(() => tree.unmount());
});
it("changes background protection inline using authoritative native state", async () => {
  const test = capability();
  const tree = await render(test);
  const toggle = tree.root.findByType("input");
  expect(toggle.props.checked).toBe(true);
  await act(async () => toggle.props.onChange({ currentTarget: { checked: false } }));
  expect(test.execute).toHaveBeenLastCalledWith({ action: "background", enabled: false });
  expect(tree.root.findByType("input").props.checked).toBe(true);
  act(() => test.update({ background: false }));
  expect(tree.root.findByType("input").props.checked).toBe(false);
  act(() => tree.unmount());
});
it("pairs through the setup screen and never asks for an address or password", async () => {
  const test = capability();
  const tree = await render(test);
  expect(tree.root.findAllByType("input").filter(input => input.props.type !== "checkbox")).toHaveLength(0);
  expect(test.execute).toHaveBeenCalledTimes(1);
  await act(async () => tree.root.findAllByType("button").find(button => button.children.includes("Pair"))!.props.onClick());
  expect(test.execute).toHaveBeenLastCalledWith({ action: "pair_setup" });
  act(() => tree.unmount());
});
const studio = { id: "a".repeat(64), name: "Studio", current: false };
const laptop = { id: "b".repeat(64), name: "Laptop", current: false };
const press = (tree: ReturnType<typeof create>, label: string, index = 0) =>
  act(async () => tree.root.findAllByType("button").filter(button => button.children.includes(label))[index]!.props.onClick());
it("lists every paired computer and switches to the one chosen", async () => {
  const test = capability();
  test.update({ paired: true, computers: [studio, laptop] });
  const tree = await render(test);
  const text = JSON.stringify(tree.toJSON());
  expect(text).toContain("Studio");
  expect(text).toContain("Laptop");
  // This phone is in use, so only the two computers offer Use.
  await press(tree, "Use", 1);
  expect(test.execute).toHaveBeenLastCalledWith({ action: "use_computer", id: laptop.id });
  act(() => test.update({ remote: true, computer: "Laptop", computers: [studio, { ...laptop, current: true }] }));
  expect(tree.root.findAllByProps({ className: "connection-status" })).toHaveLength(1);
  act(() => tree.unmount());
});
it("forgets a computer only after the user confirms", async () => {
  const test = capability();
  test.update({ remote: true, paired: true, computer: "Studio", computers: [{ ...studio, current: true }, laptop] });
  const tree = await render(test);
  await press(tree, "Forget", 0);
  expect(test.execute).toHaveBeenCalledTimes(1);
  expect(tree.root.findByProps({ role: "alertdialog" }).findByType("p").children.join("")).toContain("Work moves to this phone");
  await press(tree, "Keep");
  expect(tree.root.findAllByProps({ role: "alertdialog" })).toHaveLength(0);
  await press(tree, "Forget", 1);
  await press(tree, "Forget", 2);
  expect(test.execute).toHaveBeenLastCalledWith({ action: "forget_computer", id: laptop.id });
  act(() => tree.unmount());
});
it("shows a recoverable error instead of loading forever", async () => {
  const test = capability();
  test.value.snapshot = () => undefined;
  test.execute.mockRejectedValue(new Error("unavailable"));
  const tree = await render(test);
  expect(tree.root.findByProps({ role: "alert" }).children.join("")).toContain("did not respond");
  expect(tree.root.findAllByType("button").some(button => button.children.includes("Try again"))).toBe(true);
  act(() => tree.unmount());
});
it("switches back to this phone only when asked", async () => {
  const test = capability();
  test.update({ remote: true, paired: true, computer: "Studio", computers: [{ id: "a".repeat(64), name: "Studio", current: true }] });
  const tree = await render(test);
  expect(test.execute).toHaveBeenCalledTimes(1);
  await act(async () => tree.root.findAllByType("button").find(button => button.children.includes("Use"))!.props.onClick());
  expect(test.execute).toHaveBeenLastCalledWith({ action: "local" });
  act(() => tree.unmount());
});
it("keeps phone-only tools out of remote settings", async () => {
  const test = capability();
  test.update({ remote: true, paired: true, computer: "Studio", computers: [{ id: "a".repeat(64), name: "Studio", current: true }] });
  const tree = await render(test);
  const text = JSON.stringify(tree.toJSON());
  expect(text).toContain("Remote computer");
  expect(text).not.toContain("Continue while locked");
  expect(text).not.toContain("Reconnect Termux");
  act(() => tree.unmount());
});
