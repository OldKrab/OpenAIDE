// @vitest-environment jsdom

import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ComposerContextUsageControl, ComposerWithContextUsage } from "./ContextUsageIndicator";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: ReturnType<typeof createRoot>;
let media: EventTarget & { matches: boolean };

beforeEach(() => {
  media = Object.assign(new EventTarget(), { matches: true });
  vi.stubGlobal("matchMedia", () => media);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
});

function composer(capacity = 100_000, used = 31_000) {
  return (
    <ComposerWithContextUsage usage={{ used_tokens: used, capacity_tokens: capacity }}>
      <section className="composer">
        <div role="textbox" contentEditable suppressContentEditableWarning>Unsent draft</div>
        <div className="composer-actions"><ComposerContextUsageControl /><button>Send</button></div>
      </section>
    </ComposerWithContextUsage>
  );
}

it("shows context beside Send without an edge target, and dismisses details with Back", async () => {
  await act(async () => root.render(composer()));
  const button = container.querySelector<HTMLButtonElement>(".composer-actions .context-usage-compact")!;
  expect(button.textContent).toBe("31%");
  expect(button.getAttribute("aria-label")).toBe("Context usage: 31% used. Show details");
  expect(container.querySelector(".context-usage-edge")).toBeNull();
  await act(async () => button.click());
  expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1);
  expect(document.querySelector('[role="dialog"]')?.textContent).toContain("69.0K available");
  const back = new Event("openaide:back", { cancelable: true });
  await act(async () => window.dispatchEvent(back));
  expect(back.defaultPrevented).toBe(true);
  expect(button.getAttribute("aria-expanded")).toBe("false");
  expect(container.querySelector('[role="textbox"]')?.textContent).toBe("Unsent draft");
});

it("restores the desktop edge and closes the mobile popup on resize", async () => {
  await act(async () => root.render(composer()));
  await act(async () => container.querySelector<HTMLButtonElement>(".context-usage-compact")!.click());
  await act(async () => {
    media.matches = false;
    media.dispatchEvent(new Event("change"));
  });
  expect(container.querySelector(".context-usage-compact")).toBeNull();
  expect(container.querySelector(".context-usage-edge")).not.toBeNull();
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(container.querySelector(".context-usage-meter")?.getAttribute("aria-expanded")).toBe("false");
});

it("omits unknown capacity and clamps the warning percentage", async () => {
  await act(async () => root.render(composer(0)));
  expect(container.querySelector(".context-usage-compact")).toBeNull();
  await act(async () => root.render(composer(100, 140)));
  expect(container.querySelector(".context-usage-compact")?.textContent).toBe("100%");
  expect(container.querySelector(".context-usage-meter-critical")).not.toBeNull();
});
