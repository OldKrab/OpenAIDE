// @vitest-environment jsdom

import { act, useState } from "react";
import { createPortal } from "react-dom";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import { MobileTaskMoreProvider, MobileTaskMoreRow, MobileTaskMoreSubmenu, useMobileTaskMore } from "./MobileTaskMore";
import { MobileWorkbenchBar } from "./MobileWorkbenchBar";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;
afterEach(() => {
  act(() => root?.unmount());
  root = undefined;
  document.body.replaceChildren();
});

/** TaskView's share of the menu: rows that portal in from outside the bar. */
function PortaledRows({ agents, onAction }: { agents: boolean; onAction(): void }) {
  const { close } = useMobileTaskMore();
  return (
    <>
      <MobileTaskMoreRow
        icon={null}
        label="Terminal"
        onClick={() => {
          close();
          onAction();
        }}
        slot="terminal"
      />
      {agents ? (
        <MobileTaskMoreSubmenu icon={null} label="Agents" meta="1 running" slot="agents">
          <button onClick={close} role="menuitemradio" type="button">Main Agent</button>
        </MobileTaskMoreSubmenu>
      ) : null}
    </>
  );
}

function Bar({
  agents = true,
  onAction,
  onPolicy = async () => undefined,
  taskKey = "task_1",
}: {
  agents?: boolean;
  onAction(): void;
  onPolicy?(): Promise<void>;
  taskKey?: string;
}) {
  const [target, setTarget] = useState<HTMLElement | null>(null);
  return (
    <MobileTaskMoreProvider>
      <MobileWorkbenchBar
        navigationButtonRef={{ current: null }}
        navigationOpen={false}
        onActionsTarget={() => undefined}
        onOverflowTarget={setTarget}
        onToggleNavigation={() => undefined}
        permission={{ disabled: false, onChange: onPolicy, policy: "ask_every_time" }}
        taskKey={taskKey}
        title="Task"
      />
      {target ? createPortal(<PortaledRows agents={agents} onAction={onAction} />, target) : null}
      <p>Chat</p>
    </MobileTaskMoreProvider>
  );
}

function open(onAction: () => void = vi.fn(), onPolicy?: () => Promise<void>) {
  const host = document.body.appendChild(document.createElement("div"));
  root = createRoot(host);
  act(() => root!.render(<Bar onAction={onAction} onPolicy={onPolicy} />));
  act(() => button("More Task actions").click());
}

const button = (label: string) => Array.from(document.querySelectorAll("button"))
  .find((candidate) => candidate.textContent === label || candidate.getAttribute("aria-label") === label)!;
const menuIsOpen = () => !document.querySelector<HTMLElement>("#mobile-task-more")!.hidden;
const scrim = () => document.querySelector<HTMLElement>(".mobile-task-more-scrim");
const openAgents = () => act(() => document.querySelector<HTMLButtonElement>('[data-slot="agents"]')!.click());
const submenu = () => document.querySelector(".mobile-task-submenu");
const press = (target: Element) => act(() => {
  target.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true }));
});
const back = () => act(() => { window.dispatchEvent(new Event("openaide:back", { cancelable: true })); });

it("closes on a press outside, without that press reaching what is underneath", () => {
  open();
  expect(menuIsOpen()).toBe(true);
  // The scrim takes the press and closes on its click, so the menu must survive the pointerdown.
  press(scrim()!);
  expect(menuIsOpen()).toBe(true);
  act(() => scrim()!.click());
  expect(menuIsOpen()).toBe(false);
  expect(scrim()).toBeNull();
});

it("closes on a press on the rest of the bar", () => {
  open();
  press(document.querySelector('[aria-label="Open task navigation"]')!);
  expect(menuIsOpen()).toBe(false);
});

it("stays open for presses on its own rows and trigger", () => {
  open();
  press(document.querySelector('[data-slot="terminal"]')!);
  press(document.querySelector(".mobile-task-more-trigger")!);
  expect(menuIsOpen()).toBe(true);
});

it("closes once a row has run its action", () => {
  const onAction = vi.fn();
  open(onAction);
  act(() => button("Terminal").click());
  expect(onAction).toHaveBeenCalledOnce();
  expect(menuIsOpen()).toBe(false);
});

it("replaces the rows with a submenu, so nothing stacks", () => {
  open();
  openAgents();
  expect(submenu()?.textContent).toContain("Agents");
  expect(submenu()?.textContent).toContain("1 running");
  expect(document.querySelector(".mobile-task-more-row")).toBeNull();
  expect(document.querySelector<HTMLElement>("#mobile-task-more")!.dataset.submenu).toBe("agents");
});

it("closes everything when a submenu choice is made", () => {
  open();
  openAgents();
  act(() => button("Main Agent").click());
  expect(menuIsOpen()).toBe(false);
  act(() => button("More Task actions").click());
  expect(submenu()).toBeNull();
});

it("steps out one level on Back", () => {
  open();
  openAgents();
  back();
  expect(submenu()).toBeNull();
  expect(menuIsOpen()).toBe(true);
  back();
  expect(menuIsOpen()).toBe(false);
});

it("returns from a submenu with its own arrow and puts focus back on the row", () => {
  open();
  openAgents();
  expect(document.activeElement).toBe(button("Back to Task actions"));
  act(() => button("Back to Task actions").click());
  expect(submenu()).toBeNull();
  expect(menuIsOpen()).toBe(true);
  expect(document.activeElement).toBe(document.querySelector('[data-slot="agents"]'));
});

it("closes everything on Escape and leaves focus on the trigger", () => {
  open();
  openAgents();
  act(() => { document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); });
  expect(menuIsOpen()).toBe(false);
  expect(document.activeElement).toBe(document.querySelector(".mobile-task-more-trigger"));
});

it("closes everything from the trigger, even inside a submenu", () => {
  open();
  openAgents();
  act(() => button("Hide more Task actions").click());
  expect(menuIsOpen()).toBe(false);
  act(() => button("More Task actions").click());
  expect(submenu()).toBeNull();
});

it("keeps the menu open on the permission switch, so its new state can be read", async () => {
  let finish!: () => void;
  const onPolicy = vi.fn(() => new Promise<void>((resolve) => { finish = resolve; }));
  open(vi.fn(), onPolicy);
  const toggle = document.querySelector<HTMLButtonElement>('[role="switch"]')!;
  expect(toggle.getAttribute("aria-checked")).toBe("false");
  act(() => toggle.click());
  expect(onPolicy).toHaveBeenCalledOnce();
  expect(toggle.disabled).toBe(true);
  expect(menuIsOpen()).toBe(true);
  await act(async () => finish());
  expect(toggle.disabled).toBe(false);
});

it("does not stay in a submenu whose control went away", () => {
  const onAction = vi.fn();
  open(onAction);
  openAgents();
  act(() => root!.render(<Bar agents={false} onAction={onAction} />));
  expect(submenu()).toBeNull();
  expect(document.querySelector('[data-slot="terminal"]')).not.toBeNull();
});

it("starts closed on another Task", () => {
  const onAction = vi.fn();
  open(onAction);
  openAgents();
  act(() => root!.render(<Bar onAction={onAction} taskKey="task_2" />));
  expect(menuIsOpen()).toBe(false);
  act(() => button("More Task actions").click());
  expect(submenu()).toBeNull();
});
