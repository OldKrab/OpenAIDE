// @vitest-environment jsdom

import { act, useState } from "react";
import { createPortal } from "react-dom";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import { MobileWorkbenchBar } from "./MobileWorkbenchBar";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;
afterEach(() => {
  act(() => root?.unmount());
  root = undefined;
  document.body.replaceChildren();
});

/** The bar as TaskView fills it: a control portaled in, and a menu in the popup layer. */
function Bar({ onAction, taskKey = "task_1" }: { onAction(): void; taskKey?: string }) {
  const [target, setTarget] = useState<HTMLElement | null>(null);
  return (
    <>
      <MobileWorkbenchBar
        navigationButtonRef={{ current: null }}
        navigationOpen={false}
        onActionsTarget={() => undefined}
        onOverflowTarget={setTarget}
        onToggleNavigation={() => undefined}
        permission={{ disabled: false, onChange: async () => undefined, policy: "ask_every_time" }}
        taskKey={taskKey}
        title="Task"
      />
      {target ? createPortal(
        <>
          <button onClick={onAction} type="button">Terminal</button>
          <button data-oa-popup-trigger type="button">Switch agent</button>
        </>,
        target,
      ) : null}
      <p>Chat</p>
      <div id="openaide-popup-layer">
        <strong>Agents</strong>
        <button onClick={onAction} role="menuitemradio" type="button">Main Agent</button>
      </div>
    </>
  );
}

function open(onAction: () => void = vi.fn()) {
  const host = document.body.appendChild(document.createElement("div"));
  root = createRoot(host);
  act(() => root!.render(<Bar onAction={onAction} />));
  act(() => button("More Task actions").click());
  expect(rowIsOpen()).toBe(true);
}

const rowIsOpen = () => !document.querySelector<HTMLElement>("#mobile-task-more")!.hidden;

function button(name: string) {
  const match = [...document.querySelectorAll("button")].find((candidate) =>
    candidate.getAttribute("aria-label") === name || candidate.textContent === name);
  if (!match) throw new Error(`No button ${name}`);
  return match;
}

function press(target: Element) {
  act(() => void target.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true })));
}

it("closes on a press outside the row", () => {
  open();
  press(document.querySelector("p")!);
  expect(rowIsOpen()).toBe(false);
});

it("stays open for a press on the row or on a menu one of its controls opened", () => {
  open();
  press(document.querySelector("#mobile-task-more")!);
  press(button("Switch agent"));
  press(document.querySelector("#openaide-popup-layer strong")!);
  expect(rowIsOpen()).toBe(true);
});

it("closes once an action ran, but not when a control only opens its menu", () => {
  const onAction = vi.fn();
  open(onAction);
  act(() => button("Switch agent").click());
  expect(rowIsOpen()).toBe(true);

  act(() => button("Main Agent").click());
  expect(onAction).toHaveBeenCalledOnce();
  expect(rowIsOpen()).toBe(false);

  act(() => button("More Task actions").click());
  act(() => button("Terminal").click());
  expect(onAction).toHaveBeenCalledTimes(2);
  expect(rowIsOpen()).toBe(false);
});

it("closes on its own trigger, Back, and Escape", () => {
  open();
  act(() => button("Hide more Task actions").click());
  expect(rowIsOpen()).toBe(false);

  act(() => button("More Task actions").click());
  let unhandled = true;
  act(() => { unhandled = window.dispatchEvent(new Event("openaide:back", { cancelable: true })); });
  expect(unhandled).toBe(false);
  expect(rowIsOpen()).toBe(false);

  act(() => button("More Task actions").click());
  act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })));
  expect(rowIsOpen()).toBe(false);
});

it("leaves Back to the next surface while closed", () => {
  open();
  act(() => button("Hide more Task actions").click());
  expect(window.dispatchEvent(new Event("openaide:back", { cancelable: true }))).toBe(true);
});
