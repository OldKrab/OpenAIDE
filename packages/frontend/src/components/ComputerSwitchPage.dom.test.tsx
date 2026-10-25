// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ComputerSwitchResult, ComputersSnapshot } from "../services/computers";
import { computerSwitch } from "../services/computerSwitch";
import { WorkbenchConnectionSurface, connectionSurfaceTitle } from "./ComputerSwitchPage";

const linux = { id: "linux-key", name: "old-linux", current: true };
const oldPc = { id: "pc-key", name: "OLD-PC", current: false };
const snapshot: ComputersSnapshot = { device: "phone", computers: [linux, oldPc] };

describe("WorkbenchConnectionSurface", () => {
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

  function render(use: (id?: string) => Promise<ComputerSwitchResult>, { appServerError, cancel }: { appServerError?: string; cancel?: () => Promise<void> } = {}) {
    const switcher = computerSwitch({ snapshot: () => snapshot, subscribe: () => () => undefined, use, cancel, forget: async () => undefined });
    const closeNavigation = vi.fn();
    act(() => root.render(
      <WorkbenchConnectionSurface appServerError={appServerError} closeNavigation={closeNavigation} onRetry={vi.fn()} snapshot={snapshot} switcher={switcher}>
        <p>Task</p>
      </WorkbenchConnectionSurface>,
    ));
    return { switcher, closeNavigation };
  }
  const heading = () => container.querySelector("h2")?.textContent;
  const button = (text: string) => [...container.querySelectorAll("button")].find((candidate) => candidate.textContent === text)!;

  it("shows the wait as a page, closes the navigation, and cancels back to the computer in use", async () => {
    let answer!: (result: ComputerSwitchResult) => void;
    const cancel = vi.fn(async () => answer("cancelled"));
    const { switcher, closeNavigation } = render(() => new Promise((resolve) => { answer = resolve; }), { cancel });
    expect(container.textContent).toBe("Task");
    act(() => void switcher.request("pc-key"));
    expect(heading()).toBe("Connecting to OLD-PC…");
    expect(container.querySelector("p")?.textContent).toBe("You stay on old-linux until OLD-PC answers.");
    expect(closeNavigation).toHaveBeenCalled();
    await act(async () => button("Cancel").click());
    expect(cancel).toHaveBeenCalledOnce();
    expect(container.textContent).toBe("Task");
  });

  it("says why a switch failed, retries it, and goes back on request", async () => {
    const use = vi.fn(async (): Promise<ComputerSwitchResult> => { throw new Error("Can’t reach OLD-PC. Make sure it is on."); });
    const { switcher } = render(use);
    await act(async () => switcher.request("pc-key"));
    expect(heading()).toBe("Can’t switch to OLD-PC");
    expect(container.querySelector("p")?.textContent).toBe("Can’t reach OLD-PC. Make sure it is on.");
    await act(async () => button("Try again").click());
    expect(use).toHaveBeenCalledTimes(2);
    expect(use).toHaveBeenLastCalledWith("pc-key", expect.any(String));
    act(() => button("Back to old-linux").click());
    expect(container.textContent).toBe("Task");
  });

  it("offers the other places on the connection-problem page", async () => {
    const use = vi.fn(async (): Promise<ComputerSwitchResult> => "unchanged");
    render(use, { appServerError: "App Server WebSocket link ended: open_timeout" });
    expect(heading()).toBe("Can’t connect to old-linux");
    await act(async () => button("OLD-PC").click());
    expect(use).toHaveBeenCalledWith("pc-key", expect.any(String));
    expect(button("This phone")).toBeTruthy();
  });
});

describe("connectionSurfaceTitle", () => {
  it("names the cover instead of the Task it hides", () => {
    expect(connectionSurfaceTitle({ cancellable: true, pending: "pc-key" }, undefined)).toBe("Switching computers");
    expect(connectionSurfaceTitle({ cancellable: false, failed: { id: "pc-key", message: "Can’t reach OLD-PC." } }, "refused")).toBe("Switching computers");
    expect(connectionSurfaceTitle({ cancellable: false }, "refused")).toBe("Not connected");
    expect(connectionSurfaceTitle({ cancellable: false }, undefined)).toBeUndefined();
  });
});
