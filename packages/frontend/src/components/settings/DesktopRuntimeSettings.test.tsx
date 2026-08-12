import React from "react";
import { act, create } from "react-test-renderer";
import { describe, expect, it, vi } from "vitest";
import { DesktopRuntimeSettings } from "./DesktopRuntimeSettings";

describe("Desktop runtime settings", () => {
  it("restarts into the only usable WSL distro immediately", async () => {
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
    await act(async () => wsl.props.onClick());

    expect(select).toHaveBeenCalledWith({ kind: "wsl", distro: "Ubuntu" });
    expect(tree.root.findAllByProps({ role: "alertdialog" })).toHaveLength(0);
  });

  it("does nothing when the active environment is selected", async () => {
    const select = vi.fn(async () => undefined);
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <DesktopRuntimeSettings
          capability={{
            snapshot: () => ({ active: { kind: "wsl", distro: "Ubuntu" }, wslDistros: ["Ubuntu"] }),
            select,
          }}
        />,
      );
    });

    await act(async () => tree.root.findByProps({ "aria-label": "Use WSL" }).props.onClick());

    expect(select).not.toHaveBeenCalled();
  });

  it("requires a distro choice when more than one usable distro exists", async () => {
    const select = vi.fn(async () => undefined);
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <DesktopRuntimeSettings
          capability={{
            snapshot: () => ({ active: { kind: "native" }, wslDistros: ["Ubuntu", "Debian"] }),
            select,
          }}
        />,
      );
    });

    act(() => tree.root.findByProps({ "aria-label": "Use WSL" }).props.onClick());

    const distro = tree.root.findByProps({ "aria-label": "WSL distribution" });
    await act(async () => distro.props.onChange({ currentTarget: { value: "Debian" } }));

    expect(select).toHaveBeenCalledWith({ kind: "wsl", distro: "Debian" });
  });
});
