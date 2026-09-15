// @vitest-environment jsdom

import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { UserMessageNavigator } from "./UserMessageNavigator";
import type { UserMessageNavigation } from "./useTaskChatScroll";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("keeps phone navigation in the header, supports Back, and restores the shared layout", async () => {
  const container = document.createElement("main");
  const header = document.createElement("header");
  document.body.append(header, container);
  const root = createRoot(container);
  const scrollIntoView = vi.fn();
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  const originalScroll = HTMLElement.prototype.scrollIntoView;
  HTMLElement.prototype.scrollIntoView = scrollIntoView;
  const navigation: UserMessageNavigation = {
    anchors: [
      { key: "first", rowIndex: 0, text: "Plan the smallest safe change" },
      { key: "second", rowIndex: 2, text: "" },
    ],
    currentIndex: 1,
    hasEarlier: true,
    pendingPrevious: false,
    navigateTo: vi.fn(),
    goFirst: vi.fn(), goLast: vi.fn(), goNext: vi.fn(), goPrevious: vi.fn(),
  };
  const open = async () => {
    await act(async () => header.querySelector<HTMLButtonElement>("button")!.click());
  };
  try {
    await act(async () => root.render(<UserMessageNavigator navigation={navigation} target={header} />));
    expect(container.childElementCount).toBe(0);
    expect(header.querySelector("button")?.getAttribute("aria-label")).toBe("Jump to a message");
    await open();
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain("loaded history");
    const selected = document.querySelector<HTMLButtonElement>('[aria-current="true"]')!;
    expect(selected.textContent).toContain("Attachment-only message");
    expect(scrollIntoView).toHaveBeenCalled();
    await act(async () => selected.click());
    expect(navigation.navigateTo).toHaveBeenCalledWith(navigation.anchors[1]);
    expect(header.querySelector("button")?.getAttribute("aria-expanded")).toBe("false");
    await open();
    const back = new Event("openaide:back", { cancelable: true });
    await act(async () => window.dispatchEvent(back));
    expect(back.defaultPrevented).toBe(true);
    expect(header.querySelector("button")?.getAttribute("aria-expanded")).toBe("false");
    await open();
    const earlier = Array.from(document.querySelectorAll<HTMLButtonElement>(".user-message-picker-list button"))
      .find((button) => button.textContent === "Go to start of loaded history")!;
    await act(async () => earlier.click());
    expect(navigation.goFirst).toHaveBeenCalledOnce();
    await act(async () => root.render(<UserMessageNavigator navigation={{ ...navigation, currentIndex: 0 }} target={header} />));
    await open();
    const loadEarlier = Array.from(document.querySelectorAll<HTMLButtonElement>(".user-message-picker-list button"))
      .find((button) => button.textContent === "Go to earlier messages")!;
    await act(async () => loadEarlier.click());
    expect(navigation.goPrevious).toHaveBeenCalledOnce();
    await act(async () => root.render(<UserMessageNavigator navigation={navigation} />));
    expect(header.childElementCount).toBe(0);
    expect(container.querySelector('[aria-label="User message navigation"]')).not.toBeNull();
    expect(container.querySelector(".user-message-navigator-mobile-toggle")).toBeNull();
  } finally {
    await act(async () => root.unmount());
    header.remove();
    container.remove();
    HTMLElement.prototype.scrollIntoView = originalScroll;
    vi.unstubAllGlobals();
  }
});
