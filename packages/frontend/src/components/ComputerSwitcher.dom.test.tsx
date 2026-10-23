// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ComputersSnapshot } from "../services/computers";
import { ComputerSwitcher, computerLink, type ComputerLink } from "./ComputerSwitcher";

const studio = { id: "studio-key", name: "Studio", current: false };
const laptop = { id: "laptop-key", name: "", current: false };

describe("ComputerSwitcher", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    vi.spyOn(console, "info").mockImplementation(() => undefined);
    vi.spyOn(console, "warn").mockImplementation(() => undefined);
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
    vi.restoreAllMocks();
  });

  function render(
    snapshot: ComputersSnapshot | undefined,
    {
      link = "connected" as ComputerLink,
      onUse = vi.fn(async (_id?: string): Promise<void> => undefined),
      onManage = vi.fn(),
      openRequest = 0,
    } = {},
  ) {
    act(() => root.render(
      <ComputerSwitcher link={link} onManage={onManage} onUse={onUse} openRequest={openRequest} snapshot={snapshot} />,
    ));
    return { onUse, onManage };
  }

  const trigger = () => container.querySelector<HTMLButtonElement>("button.computer-switcher-trigger");
  const menuItems = () => Array.from(document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]'));
  const open = () => act(() => trigger()!.click());
  const isOpen = () => trigger()!.getAttribute("aria-expanded") === "true";
  // Keys reach the list from the focused row, as arrow navigation leaves them.
  const press = (key: string) => act(() => {
    menuItems()[0].dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key }));
  });

  it("stays out of the way until a computer is paired", () => {
    render(undefined);
    expect(trigger()).toBeNull();
    render({ device: "computer", computers: [] });
    expect(trigger()).toBeNull();
  });

  it("shows only an icon while this device is in use", () => {
    render({ device: "computer", computers: [studio] });
    expect(trigger()!.getAttribute("aria-label")).toBe("Working on this computer. Change computer");
    expect(trigger()!.textContent).toBe("");
  });

  it("names the computer in use, with words only for an unhealthy link", () => {
    render({ device: "computer", computers: [{ ...studio, current: true }] });
    expect(trigger()!.textContent).toBe("Studio");
    render({ device: "computer", computers: [{ ...studio, current: true }] }, { link: "reconnecting" });
    expect(trigger()!.textContent).toBe("StudioReconnecting…");
    expect(trigger()!.getAttribute("aria-label")).toBe("Working on Studio. Reconnecting…. Change computer");
  });

  it("lists this device first, then computers in the order added, with the one in use checked", () => {
    render({ device: "phone", computers: [{ ...studio, current: true }, laptop] });
    open();
    expect(menuItems().map((item) => [item.textContent, item.getAttribute("aria-checked")])).toEqual([
      ["1This phone", "false"],
      ["2Studio", "true"],
      ["3Remote computer", "false"],
      ["Pair or manage computers…", null],
    ]);
  });

  it("asks the shell to switch and shows the switch in progress", async () => {
    let accept!: () => void;
    const onUse = vi.fn(() => new Promise<void>((resolve) => { accept = resolve; }));
    render({ device: "computer", computers: [studio] }, { onUse });
    open();
    act(() => menuItems()[1].click());
    expect(onUse).toHaveBeenCalledWith("studio-key");
    expect(trigger()!.getAttribute("aria-busy")).toBe("true");
    open();
    expect(menuItems()[1].textContent).toContain("Connecting…");
    expect(menuItems().every((item) => item.getAttribute("aria-disabled") === "true" || item.textContent === "Pair or manage computers…")).toBe(true);
    await act(async () => accept());
  });

  it("switches back to this device without an id", () => {
    const { onUse } = render({ device: "computer", computers: [{ ...studio, current: true }] });
    open();
    act(() => menuItems()[0].click());
    expect(onUse).toHaveBeenCalledWith(undefined);
  });

  it("closes without switching when the computer in use is chosen", () => {
    const { onUse } = render({ device: "computer", computers: [{ ...studio, current: true }] });
    open();
    act(() => menuItems()[1].click());
    expect(onUse).not.toHaveBeenCalled();
    expect(isOpen()).toBe(false);
  });

  it("keeps the user in place with the shell's sentence when the switch is refused", async () => {
    const onUse = vi.fn(async () => { throw new Error("Studio could not be reached."); });
    render({ device: "computer", computers: [studio] }, { onUse });
    open();
    await act(async () => menuItems()[1].click());
    expect(document.querySelector('[role="menu"] [role="alert"]')!.textContent).toBe("Studio could not be reached.");
    expect(trigger()!.getAttribute("aria-busy")).toBeNull();
    await act(async () => menuItems()[1].click());
    expect(onUse).toHaveBeenCalledTimes(2);
  });

  it("picks a row by its number", () => {
    const { onUse } = render({ device: "computer", computers: [studio, laptop] });
    open();
    press("3");
    expect(onUse).toHaveBeenCalledWith("laptop-key");
  });

  it("ignores numbers without a row", () => {
    const { onUse } = render({ device: "computer", computers: [studio] });
    open();
    press("5");
    expect(onUse).not.toHaveBeenCalled();
  });

  it("leads to Settings to pair or manage", () => {
    const { onManage } = render({ device: "computer", computers: [studio] });
    open();
    act(() => menuItems().at(-1)!.click());
    expect(onManage).toHaveBeenCalledOnce();
    expect(isOpen()).toBe(false);
  });

  it("opens when a shell command asks for it", () => {
    const snapshot: ComputersSnapshot = { device: "computer", computers: [studio] };
    render(snapshot, { openRequest: 0 });
    expect(isOpen()).toBe(false);
    render(snapshot, { openRequest: 1 });
    expect(isOpen()).toBe(true);
  });

  it("reads the link from the session state", () => {
    expect(computerLink({ status: "ready" })).toBe("connected");
    expect(computerLink({ status: "ready", unresponsive: true })).toBe("notResponding");
    expect(computerLink({ status: "connecting" })).toBe("connecting");
    expect(computerLink({ status: "reconnecting", message: "" })).toBe("reconnecting");
    expect(computerLink({ status: "unavailable", message: "" })).toBe("notConnected");
  });
});
