import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DesktopTitleBar } from "./DesktopTitleBar";

describe("DesktopTitleBar", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    vi.stubGlobal("window", { addEventListener: vi.fn(), removeEventListener: vi.fn() });
  });

  it("keeps Windows chrome draggable without duplicating sidebar commands", () => {
    const windowControls = controls("windows");
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <DesktopTitleBar window={windowControls} />,
      );
    });

    expect(tree.root.findAllByProps({ "aria-haspopup": "menu" })).toHaveLength(0);
    expect(tree.root.findByProps({ "aria-label": "Window controls" })).toBeTruthy();

    act(() => tree.root.findByProps({ className: "desktop-title-bar-content" }).props.onDoubleClick(mouseEvent(2)));
    expect(windowControls.toggleMaximize).toHaveBeenCalledOnce();
  });

  it("leaves commands and caption buttons to native macOS chrome", () => {
    const windowControls = controls("macos");
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <DesktopTitleBar window={windowControls} />,
      );
    });

    expect(tree.root.findAllByProps({ "aria-haspopup": "menu" })).toHaveLength(0);
    expect(tree.root.findAllByProps({ "aria-label": "Window controls" })).toHaveLength(0);
  });

  it("prepares macOS native drag and zoom through the Desktop capability", () => {
    const windowControls = controls("macos");
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <DesktopTitleBar window={windowControls} />,
      );
    });

    const dragRegion = tree.root.findByProps({ className: "desktop-title-bar-content" });
    expect(dragRegion.props["data-tauri-drag-region"]).toBeUndefined();

    act(() => dragRegion.props.onMouseDown(mouseEvent(1)));
    act(() => dragRegion.props.onDoubleClick(mouseEvent(2)));

    expect(windowControls.startDragging).toHaveBeenCalledOnce();
    expect(windowControls.toggleMaximize).toHaveBeenCalledOnce();
  });
});

function controls(platform: "macos" | "windows") {
  return {
    platform,
    close: vi.fn(async () => undefined),
    minimize: vi.fn(async () => undefined),
    startDragging: vi.fn(async () => undefined),
    toggleMaximize: vi.fn(async () => undefined),
  };
}

function mouseEvent(detail: number) {
  const target = { closest: vi.fn(() => null) };
  return {
    button: 0,
    currentTarget: target,
    detail,
    preventDefault: vi.fn(),
    target,
  };
}
