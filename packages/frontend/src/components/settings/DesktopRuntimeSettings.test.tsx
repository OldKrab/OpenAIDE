import React from "react";
import { act, create } from "react-test-renderer";
import { describe, expect, it, vi } from "vitest";
import { DesktopRuntimeSettings } from "./DesktopRuntimeSettings";

describe("Desktop runtime settings", () => {
  it("auto-selects the only usable WSL distro after restart confirmation", () => {
    const select = vi.fn(async () => undefined);
    const confirm = vi.fn(() => true);
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <DesktopRuntimeSettings
          capability={{
            snapshot: () => ({ active: { kind: "native" }, wslDistros: ["Ubuntu"] }),
            select,
          }}
          confirm={confirm}
        />,
      );
    });

    const wsl = tree.root.findByProps({ children: "WSL" });
    act(() => wsl.props.onClick());

    expect(confirm).toHaveBeenCalledWith("Restart OpenAIDE and switch to WSL: Ubuntu?");
    expect(select).toHaveBeenCalledWith({ kind: "wsl", distro: "Ubuntu" });
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
          confirm={() => true}
        />,
      );
    });

    act(() => tree.root.findByProps({ children: "WSL" }).props.onClick());

    expect(tree.root.findByProps({ "aria-label": "WSL distribution" })).toBeTruthy();
  });
});
