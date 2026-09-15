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
    remote: false, address: "", username: "", busy: false, notice: "", termux: true, permission: true,
    background: true, appBattery: true, termuxBattery: true, batterySaver: false, notifications: true, boot: false, checks: null,
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
it("keeps remote credentials while state refreshes and verifies on submit only", async () => {
  const test = capability();
  const tree = await render(test);
  act(() => tree.root.findAllByProps({ role: "radio" })[1].props.onClick());
  const fields = tree.root.findAllByType("input").filter(input => input.props.type !== "checkbox");
  act(() => {
    fields[0].props.onChange({ currentTarget: { value: "https://computer.example" } });
    fields[1].props.onChange({ currentTarget: { value: "user" } });
    fields[2].props.onChange({ currentTarget: { value: "secret" } });
    test.update({ background: false });
  });
  expect(fields[2].props.value).toBe("secret");
  expect(test.execute).toHaveBeenCalledTimes(1);
  await act(async () => tree.root.findByType("form").props.onSubmit({ preventDefault() {} }));
  expect(test.execute).toHaveBeenLastCalledWith({ action: "remote", address: "https://computer.example", username: "user", password: "secret" });
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
it("offers keyboard choices without switching the live connection until Connect", async () => {
  const test = capability();
  test.update({ remote: true, address: "https://computer.example" });
  const tree = await render(test);
  const focus = vi.fn();
  act(() => tree.root.findByProps({ role: "radiogroup" }).props.onKeyDown({ key: "ArrowLeft", preventDefault() {}, currentTarget: { querySelectorAll: () => [{ focus }, { focus }] } }));
  const choices = tree.root.findAllByProps({ role: "radio" });
  expect(choices[0].props["aria-checked"]).toBe(true);
  expect(choices[0].props.tabIndex).toBe(0);
  expect(choices[1].props.tabIndex).toBe(-1);
  expect(choices[1].findByProps({ className: "desktop-runtime-active" }).children).toContain("Connected");
  expect(focus).toHaveBeenCalledOnce();
  expect(test.execute).toHaveBeenCalledTimes(1);
  await act(async () => tree.root.findAllByType("button").find(button => button.children.includes("Connect to this phone"))!.props.onClick());
  expect(test.execute).toHaveBeenLastCalledWith({ action: "local" });
  act(() => tree.unmount());
});
it("keeps phone-only tools out of remote settings", async () => {
  const test = capability();
  test.update({ remote: true, address: "https://computer.example" });
  const tree = await render(test);
  const text = JSON.stringify(tree.toJSON());
  expect(text).toContain("Remote work continues");
  expect(text).not.toContain("Continue while locked");
  expect(text).not.toContain("Reconnect Termux");
  act(() => tree.unmount());
});
