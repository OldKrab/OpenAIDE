// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keepTextEntryFocusOnTap } from "./mobileComposerBehavior";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;

afterEach(async () => {
  if (root) await act(async () => root?.unmount());
  root = undefined;
  document.body.replaceChildren();
  vi.unstubAllGlobals();
});

describe("keepTextEntryFocusOnTap", () => {
  it("keeps focus on the Composer when a touch taps Chat", async () => {
    await renderChat({ touch: true });
    focusComposer();

    expect(pressCancelled("Chat message")).toBe(true);
    expect(pressCancelled("Chat action")).toBe(true);
  });

  it("lets a tap move focus to a Question field", async () => {
    await renderChat({ touch: true });
    focusComposer();

    expect(pressCancelled("Question answer")).toBe(false);
  });

  it("leaves a secondary press to the platform", async () => {
    await renderChat({ touch: true });
    focusComposer();

    expect(pressCancelled("Chat message", 2)).toBe(false);
  });

  it("leaves focus alone when no text field holds it", async () => {
    await renderChat({ touch: true });

    expect(pressCancelled("Chat action")).toBe(false);
  });

  it("leaves focus and text selection alone on desktop", async () => {
    await renderChat({ touch: false });
    focusComposer();

    expect(pressCancelled("Chat message")).toBe(false);
  });
});

async function renderChat({ touch }: { touch: boolean }) {
  vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: touch })));
  const container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root?.render(<>
    <div aria-label="Message" contentEditable="plaintext-only" suppressContentEditableWarning tabIndex={0} />
    <div onMouseDown={keepTextEntryFocusOnTap}>
      <p aria-label="Chat message">Reply</p>
      <button aria-label="Chat action" type="button" />
      <input aria-label="Question answer" />
    </div>
  </>));
}

function focusComposer() {
  document.querySelector<HTMLElement>('[aria-label="Message"]')!.focus();
}

function pressCancelled(label: string, button = 0) {
  const event = new MouseEvent("mousedown", { bubbles: true, button, cancelable: true });
  act(() => {
    document.querySelector(`[aria-label="${label}"]`)!.dispatchEvent(event);
  });
  return event.defaultPrevented;
}
