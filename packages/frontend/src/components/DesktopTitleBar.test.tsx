import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DesktopTitleBar } from "./DesktopTitleBar";

describe("DesktopTitleBar", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    vi.stubGlobal("window", { addEventListener: vi.fn(), removeEventListener: vi.fn() });
  });

  it("puts frequent Windows commands and caption controls in native chrome", () => {
    const newTask = vi.fn();
    const addProject = vi.fn();
    const openSettings = vi.fn();
    const windowControls = controls("windows");
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <DesktopTitleBar
          commands={{ addProject, newTask, openSettings }}
          window={windowControls}
        >
          <strong>Current task</strong>
        </DesktopTitleBar>,
      );
    });

    act(() => tree.root.findByProps({ "aria-haspopup": "menu" }).props.onClick());
    const menu = tree.root.findByProps({ "aria-label": "OpenAIDE actions" });
    const newTaskButton = menu.findAllByType("button").find((button) =>
      button.findAll((node) => node.type === "strong" && node.children.includes("New task")).length > 0,
    );
    act(() => newTaskButton?.props.onClick());

    expect(newTask).toHaveBeenCalledOnce();
    expect(tree.root.findByProps({ "aria-label": "Window controls" })).toBeTruthy();
    expect(tree.root.findByProps({ children: "Current task" })).toBeTruthy();

    act(() => tree.root.findByProps({ className: "desktop-title-bar-content" }).props.onDoubleClick(mouseEvent(2)));
    expect(windowControls.toggleMaximize).toHaveBeenCalledOnce();
  });

  it("leaves commands and caption buttons to native macOS chrome", () => {
    const windowControls = controls("macos");
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <DesktopTitleBar commands={{ newTask: vi.fn(), openSettings: vi.fn() }} window={windowControls}>
          <strong>Current task</strong>
        </DesktopTitleBar>,
      );
    });

    expect(tree.root.findAllByProps({ "aria-haspopup": "menu" })).toHaveLength(0);
    expect(tree.root.findAllByProps({ "aria-label": "Window controls" })).toHaveLength(0);
    expect(tree.root.findByProps({ children: "Current task" })).toBeTruthy();
  });

  it("prepares macOS native drag and zoom through the Desktop capability", () => {
    const windowControls = controls("macos");
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <DesktopTitleBar commands={{ newTask: vi.fn(), openSettings: vi.fn() }} window={windowControls} />,
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
