import { act, create } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AppSidebarFrame } from "./AppSidebarFrame";

describe("AppSidebarFrame", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });
  afterEach(() => vi.unstubAllGlobals());

  it("keeps one persisted width when sidebar content changes", () => {
    const setItem = vi.fn();
    vi.stubGlobal("window", {
      localStorage: { getItem: vi.fn(() => null), setItem },
    });
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AppSidebarFrame sidebar={<nav>Tasks</nav>}>
          <article>Task</article>
        </AppSidebarFrame>,
      );
    });
    const separator = tree.root.findByProps({ role: "separator" });

    act(() => separator.props.onPointerDown({
      button: 0,
      clientX: 248,
      currentTarget: { setPointerCapture: vi.fn() },
      pointerId: 4,
      preventDefault: vi.fn(),
    }));
    act(() => separator.props.onPointerMove({ clientX: 356, pointerId: 4 }));

    expect(tree.root.findByProps({ className: "app-sidebar-frame" })
      .props.style["--app-sidebar-width"]).toBe("248px");
    expect(setItem).not.toHaveBeenCalled();

    act(() => separator.props.onPointerUp({
      currentTarget: { releasePointerCapture: vi.fn() },
      pointerId: 4,
    }));
    act(() => tree.update(
      <AppSidebarFrame sidebar={<nav>Settings</nav>}>
        <article>Settings</article>
      </AppSidebarFrame>,
    ));

    expect(tree.root.findByProps({ className: "app-sidebar-frame" })
      .props.style["--app-sidebar-width"]).toBe("356px");
    expect(setItem).toHaveBeenLastCalledWith(
      "openaide.app.sidebar",
      JSON.stringify({ collapsed: false, width: 356 }),
    );
  });

  it("prevents browser text selection when pointer resizing begins", () => {
    vi.stubGlobal("window", {
      localStorage: { getItem: vi.fn(() => null), setItem: vi.fn() },
    });
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AppSidebarFrame sidebar={<nav>Selectable sidebar text</nav>}>
          <article>Selectable page text</article>
        </AppSidebarFrame>,
      );
    });
    const preventDefault = vi.fn();

    act(() => tree.root.findByProps({ role: "separator" }).props.onPointerDown({
      button: 0,
      clientX: 248,
      currentTarget: { setPointerCapture: vi.fn() },
      pointerId: 5,
      preventDefault,
    }));

    expect(preventDefault).toHaveBeenCalledOnce();
  });

  it("collapses at the left edge and restores the previous width", () => {
    const setItem = vi.fn();
    vi.stubGlobal("window", {
      localStorage: { getItem: vi.fn(() => null), setItem },
    });
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AppSidebarFrame sidebar={<nav>Tasks</nav>}>
          <article>Task</article>
        </AppSidebarFrame>,
      );
    });

    const separator = tree.root.findByProps({ role: "separator" });
    act(() => separator.props.onPointerDown({
      button: 0,
      clientX: 248,
      currentTarget: { setPointerCapture: vi.fn() },
      pointerId: 7,
      preventDefault: vi.fn(),
    }));
    act(() => separator.props.onPointerMove({ clientX: 0, pointerId: 7 }));
    act(() => separator.props.onPointerUp({
      currentTarget: { releasePointerCapture: vi.fn() },
      pointerId: 7,
    }));

    expect(tree.root.findByProps({ className: "app-sidebar-frame sidebar-collapsed" })).toBeTruthy();
    expect(tree.root.findAllByProps({ "aria-label": "Collapse sidebar" })).toHaveLength(0);
    expect(tree.root.findAllByProps({ "aria-label": "Show sidebar" })).toHaveLength(1);
    expect(setItem).toHaveBeenLastCalledWith(
      "openaide.app.sidebar",
      JSON.stringify({ collapsed: true, width: 248 }),
    );

    act(() => tree.root.findByProps({ "aria-label": "Show sidebar" }).props.onClick());

    expect(tree.root.findByProps({ className: "app-sidebar-frame" })).toBeTruthy();
    expect(tree.root.findByProps({ className: "app-sidebar-frame" })
      .props.style["--app-sidebar-width"]).toBe("248px");
  });

  it("lets keyboard users collapse from the resize separator", () => {
    vi.stubGlobal("window", {
      localStorage: { getItem: vi.fn(() => null), setItem: vi.fn() },
    });
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AppSidebarFrame sidebar={<nav>Tasks</nav>}>
          <article>Task</article>
        </AppSidebarFrame>,
      );
    });
    const preventDefault = vi.fn();

    act(() => tree.root.findByProps({ role: "separator" }).props.onKeyDown({
      key: "Enter",
      preventDefault,
    }));

    expect(preventDefault).toHaveBeenCalledOnce();
    expect(tree.root.findByProps({ className: "app-sidebar-frame sidebar-collapsed" })).toBeTruthy();
    expect(tree.root.findByProps({ "aria-label": "Show sidebar" })).toBeTruthy();
  });

  it("keeps shell-owned window controls available when the sidebar collapses", () => {
    vi.stubGlobal("window", {
      localStorage: { getItem: vi.fn(() => null), setItem: vi.fn() },
    });
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AppSidebarFrame
          header={<header aria-label="Desktop window controls">Window controls</header>}
          sidebar={<nav>Tasks</nav>}
        >
          <article>Task</article>
        </AppSidebarFrame>,
      );
    });

    act(() => tree.root.findByProps({ role: "separator" }).props.onKeyDown({
      key: "Enter",
      preventDefault: vi.fn(),
    }));

    expect(tree.root.findByProps({ "aria-label": "Desktop window controls" })).toBeTruthy();
    expect(tree.root.findByProps({ "aria-label": "Show sidebar" })).toBeTruthy();
  });
});
