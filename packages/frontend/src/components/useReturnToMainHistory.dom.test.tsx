// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import { registerBackNavigation } from "./useBackNavigation";
import { useReturnToMainHistory } from "./useReturnToMainHistory";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;
afterEach(() => {
  act(() => root?.unmount());
  root = undefined;
  document.body.replaceChildren();
});

function History({ select, selected }: { select(subagentId?: string): void; selected?: string }) {
  useReturnToMainHistory(selected, select);
  return null;
}

function render(select: (subagentId?: string) => void, selected?: string) {
  root ??= createRoot(document.body.appendChild(document.createElement("div")));
  act(() => root!.render(<History select={select} selected={selected} />));
}

const pressBack = () => window.dispatchEvent(new Event("openaide:back", { cancelable: true }));

it("returns to the main history on Back only while a Subagent is selected", () => {
  const select = vi.fn();
  render(select);
  expect(pressBack()).toBe(true);

  render(select, "subagent_1");
  expect(pressBack()).toBe(false);
  expect(select).toHaveBeenCalledExactlyOnceWith(undefined);
});

it("lets an overlay above the history take Back first", () => {
  const select = vi.fn();
  render(select, "subagent_1");
  const settings = vi.fn();
  const removeSettings = registerBackNavigation(window, settings, 20);
  try {
    pressBack();
    expect(settings).toHaveBeenCalledOnce();
    expect(select).not.toHaveBeenCalled();
  } finally { removeSettings(); }
});
