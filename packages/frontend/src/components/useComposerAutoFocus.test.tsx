// @vitest-environment jsdom

import { act, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ComposerEditorHandle } from "./ComposerEditor";
import { useComposerAutoFocus } from "./useComposerAutoFocus";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;

afterEach(async () => {
  if (root) await act(async () => root?.unmount());
  root = undefined;
  document.body.replaceChildren();
  vi.unstubAllGlobals();
});

describe("useComposerAutoFocus", () => {
  it("restores the desktop composer after the App Shell regains focus", async () => {
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: false })));
    await renderProbe();

    const editor = document.querySelector<HTMLElement>('[aria-label="Message"]')!;
    expect(document.activeElement).toBe(editor);

    window.dispatchEvent(new Event("blur"));
    editor.blur();
    expect(document.activeElement).toBe(document.body);
    window.dispatchEvent(new Event("focus"));

    expect(document.activeElement).toBe(editor);
  });

  it("does not steal restored focus from another control", async () => {
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: false })));
    await renderProbe();

    const secondaryControl = document.querySelector<HTMLButtonElement>("button")!;
    secondaryControl.focus();
    window.dispatchEvent(new Event("blur"));
    window.dispatchEvent(new Event("focus"));

    expect(document.activeElement).toBe(secondaryControl);
  });
});

async function renderProbe() {
  const container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root?.render(<AutoFocusProbe />));
}

function AutoFocusProbe() {
  const elementRef = useRef<HTMLDivElement | null>(null);
  const editorRef = useRef<ComposerEditorHandle | null>(null);
  editorRef.current ??= {
    focus: () => elementRef.current?.focus(),
  } as ComposerEditorHandle;
  useComposerAutoFocus({ autoFocus: true, disabled: false, editorRef });
  return <>
    <div aria-label="Message" ref={elementRef} tabIndex={0} />
    <button type="button">Secondary control</button>
  </>;
}
