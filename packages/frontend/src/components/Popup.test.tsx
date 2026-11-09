// @vitest-environment jsdom

import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished } from "vitest";
import { anchoredPopupMaxHeight, PopupDialog, PopupMenu, swallowTouchDismissClick } from "./Popup";

it("limits an upward popup to the space above its trigger", () => {
  expect(anchoredPopupMaxHeight({
    availableHeight: 464,
    placement: "top-start",
    referenceBottom: 295,
    referenceTop: 265,
    viewportHeight: 480,
  })).toBe(253);
});

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.replaceChildren();
});

it("owns trigger semantics and renders a labelled menu through one interface", async () => {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  await act(async () => root.render(<TestMenu />));

  const trigger = document.querySelector<HTMLButtonElement>('[aria-label="Task actions"]')!;
  expect(trigger.getAttribute("aria-expanded")).toBe("false");

  await act(async () => trigger.click());

  expect(trigger.getAttribute("aria-expanded")).toBe("true");
  const menu = document.querySelector<HTMLElement>('[role="menu"]')!;
  expect(menu.getAttribute("aria-label")).toBe("Task actions");
  expect(menu.closest("#openaide-popup-layer")).not.toBeNull();
  expect(menu.querySelectorAll('[role="menuitem"]')).toHaveLength(2);
});

it("restores the trigger on Escape", async () => {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  await act(async () => root.render(<TestMenu />));
  const trigger = document.querySelector<HTMLButtonElement>('[aria-label="Task actions"]')!;

  await act(async () => trigger.click());
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  await act(async () => {
    document.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Escape" }));
  });
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
  // Escape may finish the close transition before act returns on a busy runner.
  // Observe dismissal rather than requiring an intermediate animation frame.
  await expect.poll(async () => {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    return document.querySelector('[role="menu"]');
  }).toBeNull();
  expect(document.activeElement).toBe(trigger);
});

it("stays anchored when an open menu's trigger element is replaced", async () => {
  // jsdom reports a 0x0 viewport, which would clamp every position to the padding.
  const html = document.documentElement;
  Object.defineProperty(html, "clientWidth", { configurable: true, value: 1024 });
  Object.defineProperty(html, "clientHeight", { configurable: true, value: 768 });
  onTestFinished(() => {
    delete (html as { clientWidth?: number }).clientWidth;
    delete (html as { clientHeight?: number }).clientHeight;
  });
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  await act(async () => root.render(<SwappingTriggerMenu locked={false} />));

  // A grouped composer menu stays open while its trigger locks into a span.
  await act(async () => root.render(<SwappingTriggerMenu locked />));

  const menu = document.querySelector<HTMLElement>('[role="menu"]')!;
  await expect.poll(async () => {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    return menu.style.transform;
  }).toBe("translate(100px, 596px)");
});

it("traps modal content and dismisses only through the dialog boundary", async () => {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  await act(async () => root.render(<TestDialog />));
  const launch = document.querySelector<HTMLButtonElement>('[aria-label="Open preview"]')!;
  launch.focus();

  await act(async () => launch.click());
  const dialog = document.querySelector<HTMLElement>('[role="dialog"]')!;
  expect(dialog.getAttribute("aria-modal")).toBe("true");

  await act(async () => {
    document.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Escape" }));
  });
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(document.activeElement).toBe(launch);
});

function TestMenu() {
  const [open, setOpen] = useState(false);
  return (
    <PopupMenu
      label="Task actions"
      onOpenChange={setOpen}
      open={open}
      trigger={(props) => <button {...props} aria-label="Task actions" type="button">Actions</button>}
    >
      <button role="menuitem" type="button">Open</button>
      <button role="menuitem" type="button">Archive</button>
    </PopupMenu>
  );
}

function TestDialog() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button aria-label="Open preview" onClick={() => setOpen(true)} type="button">Open</button>
      <PopupDialog label="Image preview" onOpenChange={setOpen} open={open}>
        <button onClick={() => setOpen(false)} type="button">Close</button>
      </PopupDialog>
    </>
  );
}

function SwappingTriggerMenu({ locked }: { locked: boolean }) {
  return (
    <PopupMenu
      label="Options"
      onOpenChange={() => undefined}
      open
      placement="top-start"
      trigger={(props) => locked ? (
        <span
          ref={(node) => {
            if (node) node.getBoundingClientRect = () => new DOMRect(100, 600, 80, 30);
            props.ref(node);
          }}
        >
          Options
        </span>
      ) : <button {...props} aria-label="Options" type="button">Options</button>}
    >
      <button role="menuitem" type="button">Model</button>
    </PopupMenu>
  );
}

it("returns from an inner view on the platform Back action and closes only from the first view", async () => {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  onTestFinished(() => act(async () => root.unmount()));
  function MenuWithDetails() {
    const [open, setOpen] = useState(true);
    const [details, setDetails] = useState(true);
    return (
      <PopupMenu
        label="Task actions"
        onBack={details ? () => setDetails(false) : undefined}
        onOpenChange={setOpen}
        open={open}
        trigger={(props) => <button {...props} aria-label="Task actions" type="button" />}
      >
        {details ? <p>Details</p> : <button role="menuitem" type="button">Rename</button>}
      </PopupMenu>
    );
  }
  await act(async () => root.render(<MenuWithDetails />));
  const back = () => act(async () => { window.dispatchEvent(new Event("openaide:back", { cancelable: true })); });

  await back();
  expect(document.querySelector('[role="menu"]')?.textContent).toBe("Rename");
  await back();
  expect(document.querySelector('button[aria-label="Task actions"]')?.getAttribute("aria-expanded")).toBe("false");
});

it("lets a touch outside a menu only close it, without also clicking what lies under it", () => {
  const button = document.createElement("button");
  let clicks = 0;
  button.addEventListener("click", () => { clicks += 1; });
  document.body.append(button);

  swallowTouchDismissClick(new PointerEvent("pointerdown", { pointerType: "touch" }));
  button.click();
  expect(clicks).toBe(0);
  button.click();
  expect(clicks).toBe(1);

  swallowTouchDismissClick(new PointerEvent("pointerdown", { pointerType: "mouse" }));
  button.click();
  expect(clicks).toBe(2);
});
