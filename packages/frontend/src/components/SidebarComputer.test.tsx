import { act, create } from "react-test-renderer";
import { beforeEach, expect, it, vi } from "vitest";
import type { ConnectionSnapshot } from "../services/connectionSettings";
import type { FrontendShell } from "../services/frontendShell";
import { SidebarComputer, computerLink, useComputerInUse, type ComputerInUse } from "./SidebarComputer";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

function inUse(shell: Partial<FrontendShell>) {
  let seen: ComputerInUse | undefined;
  function Probe() {
    seen = useComputerInUse(shell as FrontendShell);
    return null;
  }
  act(() => { create(<Probe />); });
  return { seen: () => seen };
}

function phone(initial: Partial<ConnectionSnapshot>) {
  let snapshot = initial as ConnectionSnapshot;
  const listeners = new Set<() => void>();
  return {
    capability: {
      snapshot: () => snapshot,
      subscribe: (listener: () => void) => { listeners.add(listener); return () => listeners.delete(listener); },
      execute: vi.fn(),
    },
    set(next: Partial<ConnectionSnapshot>) {
      snapshot = next as ConnectionSnapshot;
      listeners.forEach((listener) => listener());
    },
  };
}

it("names no computer while this device does the work", () => {
  expect(inUse({}).seen()).toBeUndefined();
  expect(inUse({ remoteComputers: { list: vi.fn(), pair: vi.fn(), use: vi.fn(), forget: vi.fn() } }).seen()).toBeUndefined();
  expect(inUse({ connectionSettings: phone({ remote: false, computer: "Studio" }).capability }).seen()).toBeUndefined();
});

it("names the Desktop's paired computer and its Settings tab", () => {
  const shell = { remoteComputers: { current: "Studio", list: vi.fn(), pair: vi.fn(), use: vi.fn(), forget: vi.fn() } };
  expect(inUse(shell).seen()).toEqual({ name: "Studio", settingsTab: "desktop" });
});

it("follows the phone's choice of computer as it changes", () => {
  const source = phone({ remote: true, computer: "Studio" });
  const probe = inUse({ connectionSettings: source.capability });
  expect(probe.seen()).toEqual({ name: "Studio", settingsTab: "connection" });
  act(() => source.set({ remote: true, computer: "" }));
  expect(probe.seen()).toEqual({ name: "Another computer", settingsTab: "connection" });
  act(() => source.set({ remote: false, computer: "" }));
  expect(probe.seen()).toBeUndefined();
});

it("reads the link from the session state", () => {
  expect(computerLink({ status: "ready" })).toBe("connected");
  expect(computerLink({ status: "ready", unresponsive: true })).toBe("notResponding");
  expect(computerLink({ status: "connecting" })).toBe("connecting");
  expect(computerLink({ status: "reconnecting", message: "" })).toBe("reconnecting");
  expect(computerLink({ status: "unavailable", message: "" })).toBe("notConnected");
});

it("shows the computer and its link, and opens the place that changes it", () => {
  const onOpen = vi.fn();
  let tree!: ReturnType<typeof create>;
  act(() => { tree = create(<SidebarComputer link="reconnecting" name="Studio" onOpen={onOpen} />); });
  const button = tree.root.findByType("button");
  expect(button.props["data-link"]).toBe("reconnecting");
  expect(button.props["aria-label"]).toBe("Working on Studio. Reconnecting…. Change computer");
  expect(tree.root.findByProps({ className: "sidebar-computer-link" }).children).toEqual(["Reconnecting…"]);
  act(() => button.props.onClick());
  expect(onOpen).toHaveBeenCalledOnce();
});

it("lets the dot alone speak for a healthy link", () => {
  let tree!: ReturnType<typeof create>;
  act(() => { tree = create(<SidebarComputer link="connected" name="Studio" onOpen={vi.fn()} />); });
  expect(tree.root.findAllByProps({ className: "sidebar-computer-link" })).toHaveLength(0);
  expect(tree.root.findAllByProps({ className: "sidebar-computer-dot" })).toHaveLength(1);
});
