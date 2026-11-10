// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AppSidebarFrame } from "./AppSidebarFrame";

describe("AppSidebarFrame browser behavior", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    window.localStorage.clear();
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    Object.defineProperties(HTMLElement.prototype, {
      setPointerCapture: { configurable: true, value: vi.fn() },
      releasePointerCapture: { configurable: true, value: vi.fn() },
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
    delete (HTMLElement.prototype as Partial<HTMLElement>).setPointerCapture;
    delete (HTMLElement.prototype as Partial<HTMLElement>).releasePointerCapture;
    vi.restoreAllMocks();
  });

  it("restores the previous width after the sidebar is hidden by dragging", () => {
    act(() => root.render(
      <AppSidebarFrame collapsible sidebar={<nav>Tasks</nav>}>
        <article>Task</article>
      </AppSidebarFrame>,
    ));

    const frame = container.querySelector<HTMLElement>(".app-sidebar-frame");
    const previousWidth = frame?.style.getPropertyValue("--app-sidebar-width");
    expect(previousWidth).toBeTruthy();
    const separator = container.querySelector<HTMLElement>("[role=separator]");
    expect(separator).not.toBeNull();
    dispatchPointer(separator!, "pointerdown", { button: 0, clientX: 248, pointerId: 7 });
    dispatchPointer(separator!, "pointermove", { clientX: 0, pointerId: 7 });
    dispatchPointer(separator!, "pointerup", { clientX: 0, pointerId: 7 });

    const showSidebar = container.querySelector<HTMLButtonElement>("[aria-label='Show sidebar']");
    expect(showSidebar).not.toBeNull();
    act(() => showSidebar!.click());

    expect(frame?.style.getPropertyValue("--app-sidebar-width")).toBe(previousWidth);
  });

  it("animates only a toggle, and keeps focus on the toggle across it", () => {
    act(() => root.render(
      <AppSidebarFrame collapsible sidebar={<nav>Tasks</nav>}>
        <article>Task</article>
      </AppSidebarFrame>,
    ));
    const frame = container.querySelector<HTMLElement>(".app-sidebar-frame")!;
    const toggle = container.querySelector<HTMLButtonElement>(".app-sidebar-toggle")!;
    toggle.focus();

    act(() => toggle.click());

    expect(frame.classList.contains("sidebar-collapsed")).toBe(true);
    expect(frame.dataset.sidebarMotion).toBe("true");
    expect(container.querySelector(".app-sidebar-toggle")).toBe(toggle);
    expect(document.activeElement).toBe(toggle);

    const ended = new Event("transitionend");
    Object.defineProperty(ended, "propertyName", { value: "--app-sidebar-track" });
    act(() => frame.dispatchEvent(ended));
    expect(frame.dataset.sidebarMotion).toBeUndefined();

    act(() => toggle.click());
    const separator = container.querySelector<HTMLElement>("[role=separator]")!;
    dispatchPointer(separator, "pointerdown", { button: 0, clientX: 248, pointerId: 3 });
    expect(frame.dataset.sidebarMotion).toBeUndefined();
    dispatchPointer(separator, "pointermove", { clientX: 0, pointerId: 3 });
    dispatchPointer(separator, "pointerup", { clientX: 0, pointerId: 3 });
    expect(frame.classList.contains("sidebar-collapsed")).toBe(true);
    expect(frame.dataset.sidebarMotion).toBeUndefined();
  });
});

function dispatchPointer(
  target: HTMLElement,
  type: "pointerdown" | "pointermove" | "pointerup",
  init: MouseEventInit & { pointerId: number },
) {
  const event = new MouseEvent(type, { bubbles: true, ...init });
  Object.defineProperty(event, "pointerId", { value: init.pointerId });
  act(() => target.dispatchEvent(event));
}
