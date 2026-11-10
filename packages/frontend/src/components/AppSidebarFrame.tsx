import { PanelLeft } from "lucide-react";
import {
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ComponentPropsWithoutRef,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  type RefObject,
} from "react";
import { applySidebarWidth, setLayoutResizing } from "./layoutResize";

const STORAGE_KEY = "openaide.app.sidebar";
const LEGACY_SETTINGS_STORAGE_KEY = "openaide.settings.sidebar";
const MIN_WIDTH = 220;
const MAX_WIDTH = 420;
const COLLAPSE_THRESHOLD = 48;

type SidebarFrameState = {
  collapsed: boolean;
  width: number;
};

type AppSidebarFrameProps = Omit<ComponentPropsWithoutRef<"main">, "children"> & {
  children: ReactNode;
  /**
   * Lets the user hide the sidebar with a toggle, the resizer's left edge, or
   * Enter/Home on the resizer. A frame that is not collapsible always shows its
   * sidebar and leaves the shared collapse preference untouched.
   */
  collapsible?: boolean;
  header?: ReactNode;
  headerPlacement?: "overlay" | "row";
  sidebar: ReactNode;
};

/**
 * Owns the stable application sidebar frame. Sidebar contents can change by
 * surface without changing the user's width or collapse preference.
 *
 * The toggle stays mounted in one place for both states, so it keeps keyboard
 * focus across a toggle. Only the toggle animates the column; a drag already
 * moved it, so a drag to the edge snaps closed.
 */
export function AppSidebarFrame({
  children,
  className,
  collapsible = false,
  header,
  headerPlacement = "row",
  sidebar,
  style,
  ...rootProps
}: AppSidebarFrameProps) {
  const frameRef = useRef<HTMLElement>(null);
  const sidebarState = useAppSidebarState(frameRef, collapsible);
  const paneId = useId();
  const collapsed = collapsible && sidebarState.collapsed;
  const toggleLabel = collapsed ? "Show sidebar" : "Hide sidebar";
  return (
    <main
      {...rootProps}
      ref={frameRef}
      className={[
        "app-sidebar-frame",
        collapsible ? "app-sidebar-frame-collapsible" : undefined,
        header && headerPlacement === "row" ? "app-sidebar-frame-with-header" : undefined,
        header && headerPlacement === "overlay" ? "app-sidebar-frame-with-overlay-header" : undefined,
        collapsed ? "sidebar-collapsed" : undefined,
        className,
      ].filter(Boolean).join(" ")}
      style={{
        ...style,
        "--app-sidebar-width": `${sidebarState.width}px`,
      } as React.CSSProperties}
    >
      {header ? (
        <div className={headerPlacement === "overlay"
          ? "app-sidebar-frame-overlay-header"
          : "app-sidebar-frame-header"}
        >
          {header}
        </div>
      ) : null}
      {collapsible ? (
        <button
          aria-controls={paneId}
          aria-expanded={!collapsed}
          aria-label={toggleLabel}
          className="app-sidebar-toggle"
          onClick={sidebarState.toggle}
          title={toggleLabel}
          type="button"
        >
          <PanelLeft aria-hidden="true" size={16} strokeWidth={1.75} />
        </button>
      ) : null}
      <div className="app-sidebar-pane" id={paneId}>{sidebar}</div>
      {!collapsed ? (
        <>
          <div
            aria-label="Resize sidebar"
            aria-orientation="vertical"
            aria-valuemax={MAX_WIDTH}
            aria-valuemin={0}
            aria-valuenow={sidebarState.width}
            className="app-sidebar-resizer"
            onKeyDown={sidebarState.resizeWithKeyboard}
            onPointerCancel={sidebarState.cancelResize}
            onPointerDown={sidebarState.beginResize}
            onPointerMove={sidebarState.resize}
            onPointerUp={sidebarState.finishResize}
            role="separator"
            title={collapsible ? "Drag to resize. Drag to the left edge to hide." : "Drag to resize."}
            tabIndex={0}
          />
        </>
      ) : null}
      {children}
    </main>
  );
}

function useAppSidebarState(frameRef: RefObject<HTMLElement | null>, collapsible: boolean) {
  const [state, setState] = useState<SidebarFrameState>(readState);
  const stateRef = useRef(state);
  const dragRef = useRef<{
    liveWidth: number;
    pointerId: number;
    startWidth: number;
    startX: number;
  } | undefined>(undefined);

  const update = (next: SidebarFrameState, persist = true) => {
    stateRef.current = next;
    setState(next);
    if (persist) writeState(next);
  };

  useLayoutEffect(() => {
    const drag = dragRef.current;
    const frame = frameRef.current;
    if (!drag || !frame) return;
    applySidebarWidth(frame, drag.liveWidth);
    setLayoutResizing(frame, true);
  });
  const beginResize = (event: PointerEvent<HTMLElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    setSidebarMotion(frameRef.current, false);
    dragRef.current = {
      liveWidth: stateRef.current.width,
      pointerId: event.pointerId,
      startWidth: stateRef.current.width,
      startX: event.clientX,
    };
    setLayoutResizing(frameRef.current, true);
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const resize = (event: PointerEvent<HTMLElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const offset = drag.startWidth + event.clientX - drag.startX;
    const width = collapsible ? clampDragWidth(offset) : clampWidth(offset);
    drag.liveWidth = width;
    applySidebarWidth(frameRef.current, width);
  };
  const finishResize = (event: PointerEvent<HTMLElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    event.currentTarget.releasePointerCapture(event.pointerId);
    dragRef.current = undefined;
    setLayoutResizing(frameRef.current, false);
    if (collapsible && drag.liveWidth <= COLLAPSE_THRESHOLD) {
      // React still considers the remembered width current, so restore the
      // CSS variable after the imperative drag before hiding the column.
      applySidebarWidth(frameRef.current, drag.startWidth);
      update({ collapsed: true, width: drag.startWidth });
      return;
    }
    // The resizer only shows while the sidebar does; a frame that is not
    // collapsible keeps the shared preference it was given.
    update({ ...stateRef.current, width: clampWidth(drag.liveWidth) });
  };
  const cancelResize = (event: PointerEvent<HTMLElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    event.currentTarget.releasePointerCapture(event.pointerId);
    dragRef.current = undefined;
    applySidebarWidth(frameRef.current, drag.startWidth);
    setLayoutResizing(frameRef.current, false);
    update({ ...stateRef.current, width: drag.startWidth }, false);
  };
  const resizeWithKeyboard = (event: KeyboardEvent<HTMLElement>) => {
    if (collapsible && (event.key === "Enter" || event.key === "Home")) {
      event.preventDefault();
      setSidebarMotion(frameRef.current, true);
      update({ ...stateRef.current, collapsed: true });
      return;
    }
    const delta = event.key === "ArrowLeft" ? -16 : event.key === "ArrowRight" ? 16 : undefined;
    const absoluteWidth = event.key === "End" ? MAX_WIDTH : event.key === "Home" ? MIN_WIDTH : undefined;
    if (delta === undefined && absoluteWidth === undefined) return;
    event.preventDefault();
    update({
      ...stateRef.current,
      width: clampWidth(absoluteWidth ?? stateRef.current.width + delta!),
    });
  };

  return {
    ...state,
    beginResize,
    cancelResize,
    toggle: () => {
      setSidebarMotion(frameRef.current, true);
      update({ ...stateRef.current, collapsed: !stateRef.current.collapsed });
    },
    finishResize,
    resize,
    resizeWithKeyboard,
  };
}

/**
 * Marks the frame so CSS animates the next column change. The mark lasts until
 * that transition ends, so live resizing and drag-to-hide stay immediate. It is
 * cleared on transitionend because the column change happens in CSS, after
 * React has committed.
 */
function setSidebarMotion(frame: HTMLElement | null, moving: boolean) {
  if (!frame) return;
  if (!moving) {
    delete frame.dataset.sidebarMotion;
    return;
  }
  // A pending mark already has its listeners; reduced motion never transitions.
  if (frame.dataset.sidebarMotion || prefersReducedMotion()) return;
  frame.dataset.sidebarMotion = "true";
  const settle = (event: TransitionEvent) => {
    if (event.target !== frame || event.propertyName !== "--app-sidebar-track") return;
    delete frame.dataset.sidebarMotion;
    frame.removeEventListener("transitionend", settle);
    frame.removeEventListener("transitioncancel", settle);
  };
  frame.addEventListener("transitionend", settle);
  frame.addEventListener("transitioncancel", settle);
}

function prefersReducedMotion() {
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function readState(): SidebarFrameState {
  const fallback = { collapsed: false, width: defaultWidth() };
  if (typeof window === "undefined") return fallback;
  try {
    const serialized = window.localStorage.getItem(STORAGE_KEY)
      ?? window.localStorage.getItem(LEGACY_SETTINGS_STORAGE_KEY);
    const stored = JSON.parse(serialized ?? "null") as Partial<SidebarFrameState> | null;
    if (!stored) return fallback;
    return {
      collapsed: stored.collapsed === true,
      width: clampWidth(stored.width ?? fallback.width),
    };
  } catch {
    return fallback;
  }
}

function writeState(state: SidebarFrameState) {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // The frame remains usable when a host blocks local storage.
  }
}

function defaultWidth() {
  return typeof document !== "undefined" && document.body.dataset.shell === "web" ? 304 : 248;
}

function clampWidth(width: number) {
  return Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, Math.round(width)));
}

function clampDragWidth(width: number) {
  return Math.min(MAX_WIDTH, Math.max(0, Math.round(width)));
}
