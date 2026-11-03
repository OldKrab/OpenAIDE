// @vitest-environment jsdom

import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { AgentDisableDialog } from "./AgentDisableDialog";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("escapes a transformed, clipped surface so a narrow Sidebar cannot clip the confirmation", async () => {
  // Mirrors the narrow-screen Sidebar: a transformed scroll container that clips overflow. A
  // fixed overlay left inside it is positioned against and clipped by that container.
  const surface = document.createElement("aside");
  surface.style.transform = "translateX(0)";
  surface.style.overflow = "hidden";
  surface.style.width = "288px";
  document.body.append(surface);
  const root = createRoot(surface);

  try {
    await act(async () => root.render(
      <AgentDisableDialog agentLabel="Codex" onCancel={vi.fn()} onConfirm={vi.fn()} runningTaskCount={2} />,
    ));

    const dialog = document.querySelector('[role="dialog"]');
    expect(dialog).not.toBeNull();
    expect(surface.querySelector('[role="dialog"]')).toBeNull();
    expect(document.body.contains(dialog)).toBe(true);
  } finally {
    await act(async () => root.unmount());
    surface.remove();
  }
});

it("cancels on the platform Back action instead of leaving it to the surface behind", async () => {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  const onCancel = vi.fn();
  try {
    await act(async () => root.render(
      <AgentDisableDialog agentLabel="Codex" onCancel={onCancel} onConfirm={vi.fn()} runningTaskCount={1} />,
    ));
    const back = new Event("openaide:back", { cancelable: true });
    window.dispatchEvent(back);
    expect(back.defaultPrevented).toBe(true);
    expect(onCancel).toHaveBeenCalledTimes(1);
  } finally {
    await act(async () => root.unmount());
    container.remove();
  }
});
