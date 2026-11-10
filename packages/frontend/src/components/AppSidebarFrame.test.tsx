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
        <AppSidebarFrame collapsible sidebar={<nav>Tasks</nav>}>
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

    expect(tree.root.findByProps({ className: "app-sidebar-frame app-sidebar-frame-collapsible" })
      .props.style["--app-sidebar-width"]).toBe("248px");
    expect(setItem).not.toHaveBeenCalled();

    act(() => separator.props.onPointerUp({
      currentTarget: { releasePointerCapture: vi.fn() },
      pointerId: 4,
    }));
    act(() => tree.update(
      <AppSidebarFrame collapsible sidebar={<nav>Settings</nav>}>
        <article>Settings</article>
      </AppSidebarFrame>,
    ));

    expect(tree.root.findByProps({ className: "app-sidebar-frame app-sidebar-frame-collapsible" })
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
        <AppSidebarFrame collapsible sidebar={<nav>Tasks</nav>}>
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

    expect(tree.root.findByProps({ className: "app-sidebar-frame app-sidebar-frame-collapsible sidebar-collapsed" })).toBeTruthy();
    expect(tree.root.findAllByProps({ "aria-label": "Hide sidebar" })).toHaveLength(0);
    expect(tree.root.findAllByProps({ "aria-label": "Show sidebar" })).toHaveLength(1);
    expect(setItem).toHaveBeenLastCalledWith(
      "openaide.app.sidebar",
      JSON.stringify({ collapsed: true, width: 248 }),
    );

    act(() => tree.root.findByProps({ "aria-label": "Show sidebar" }).props.onClick());

    expect(tree.root.findByProps({ className: "app-sidebar-frame app-sidebar-frame-collapsible" })
      .props.style["--app-sidebar-width"]).toBe("248px");
  });

  it("lets keyboard users collapse from the resize separator", () => {
    vi.stubGlobal("window", {
      localStorage: { getItem: vi.fn(() => null), setItem: vi.fn() },
    });
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AppSidebarFrame collapsible sidebar={<nav>Tasks</nav>}>
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
    expect(tree.root.findByProps({ className: "app-sidebar-frame app-sidebar-frame-collapsible sidebar-collapsed" })).toBeTruthy();
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
          collapsible
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

  it("toggles the sidebar from one control that keeps its place", () => {
    const setItem = vi.fn();
    vi.stubGlobal("window", {
      localStorage: { getItem: vi.fn(() => null), setItem },
    });
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AppSidebarFrame collapsible sidebar={<nav>Tasks</nav>}>
          <article>Task</article>
        </AppSidebarFrame>,
      );
    });
    const toggle = () => tree.root.findByProps({ className: "app-sidebar-toggle" });
    const pane = tree.root.findByProps({ className: "app-sidebar-pane" });

    expect(toggle().props["aria-label"]).toBe("Hide sidebar");
    expect(toggle().props["aria-expanded"]).toBe(true);
    expect(toggle().props["aria-controls"]).toBe(pane.props.id);

    act(() => toggle().props.onClick());

    expect(toggle().props["aria-label"]).toBe("Show sidebar");
    expect(toggle().props["aria-expanded"]).toBe(false);
    expect(tree.root.findAllByProps({ role: "separator" })).toHaveLength(0);
    expect(setItem).toHaveBeenLastCalledWith(
      "openaide.app.sidebar",
      JSON.stringify({ collapsed: true, width: 248 }),
    );

    act(() => toggle().props.onClick());

    expect(toggle().props["aria-label"]).toBe("Hide sidebar");
    expect(tree.root.findAllByProps({ role: "separator" })).toHaveLength(1);
  });

  it("always shows a sidebar that is not collapsible without dropping the shared preference", () => {
    const setItem = vi.fn();
    vi.stubGlobal("window", {
      localStorage: {
        getItem: vi.fn(() => JSON.stringify({ collapsed: true, width: 300 })),
        setItem,
      },
    });
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <AppSidebarFrame sidebar={<nav>Settings</nav>}>
          <article>Settings</article>
        </AppSidebarFrame>,
      );
    });

    expect(tree.root.findByProps({ className: "app-sidebar-frame" })).toBeTruthy();
    expect(tree.root.findAllByProps({ className: "app-sidebar-toggle" })).toHaveLength(0);

    const separator = tree.root.findByProps({ role: "separator" });
    act(() => separator.props.onKeyDown({ key: "Enter", preventDefault: vi.fn() }));
    expect(tree.root.findByProps({ className: "app-sidebar-frame" })).toBeTruthy();

    act(() => separator.props.onPointerDown({
      button: 0,
      clientX: 300,
      currentTarget: { setPointerCapture: vi.fn() },
      pointerId: 9,
      preventDefault: vi.fn(),
    }));
    act(() => separator.props.onPointerMove({ clientX: 0, pointerId: 9 }));
    act(() => separator.props.onPointerUp({
      currentTarget: { releasePointerCapture: vi.fn() },
      pointerId: 9,
    }));

    expect(tree.root.findByProps({ className: "app-sidebar-frame" })
      .props.style["--app-sidebar-width"]).toBe("220px");
    expect(setItem).toHaveBeenLastCalledWith(
      "openaide.app.sidebar",
      JSON.stringify({ collapsed: true, width: 220 }),
    );
  });
});
