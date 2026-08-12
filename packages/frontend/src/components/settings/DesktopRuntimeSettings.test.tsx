import React from "react";
import { act, create } from "react-test-renderer";
import { describe, expect, it, vi } from "vitest";
import { DesktopRuntimeSettings } from "./DesktopRuntimeSettings";

describe("Desktop runtime settings", () => {
  it("stages the only usable WSL distro until restart is explicitly confirmed", async () => {
    const select = vi.fn(async () => undefined);
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <DesktopRuntimeSettings
          capability={{
            snapshot: () => ({ active: { kind: "native" }, wslDistros: ["Ubuntu"] }),
            select,
          }}
        />,
      );
    });

    const wsl = tree.root.findByProps({ "aria-label": "Use WSL" });
    act(() => wsl.props.onClick());

    expect(select).not.toHaveBeenCalled();
    expect(tree.root.findByProps({ role: "alertdialog" })).toBeTruthy();
    expect(tree.root.findByProps({ id: "desktop-runtime-confirm-title" }).props.children.join(""))
      .toBe("Switch to WSL · Ubuntu?");

    await act(async () => tree.root.findByProps({ children: "Restart and switch" }).props.onClick());
    expect(select).toHaveBeenCalledWith({ kind: "wsl", distro: "Ubuntu" });
  });

  it("keeps the active environment when restart confirmation is cancelled", () => {
    const select = vi.fn(async () => undefined);
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <DesktopRuntimeSettings
          capability={{
            snapshot: () => ({ active: { kind: "native" }, wslDistros: ["Ubuntu"] }),
            select,
          }}
        />,
      );
    });

    act(() => tree.root.findByProps({ "aria-label": "Use WSL" }).props.onClick());
    act(() => tree.root.findByProps({ children: "Cancel" }).props.onClick());

    expect(select).not.toHaveBeenCalled();
    expect(tree.root.findAllByProps({ role: "alertdialog" })).toHaveLength(0);
  });

  it("requires a distro choice when more than one usable distro exists", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <DesktopRuntimeSettings
          capability={{
            snapshot: () => ({ active: { kind: "native" }, wslDistros: ["Ubuntu", "Debian"] }),
            select: vi.fn(async () => undefined),
          }}
        />,
      );
    });

    act(() => tree.root.findByProps({ "aria-label": "Use WSL" }).props.onClick());

    expect(tree.root.findByProps({ "aria-label": "WSL distribution" })).toBeTruthy();
  });
});
