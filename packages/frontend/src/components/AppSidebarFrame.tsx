import { ChevronRight } from "lucide-react";
import {
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
  header?: ReactNode;
  headerPlacement?: "overlay" | "row";
  sidebar: ReactNode;
};

/**
 * Owns the stable application sidebar frame. Sidebar contents can change by
 * surface without changing the user's width or collapse preference.
 */
export function AppSidebarFrame({
  children,
  className,
  header,
  headerPlacement = "row",
  sidebar,
  style,
  ...rootProps
}: AppSidebarFrameProps) {
  const frameRef = useRef<HTMLElement>(null);
  const sidebarState = useAppSidebarState(frameRef);
  return (
    <main
      {...rootProps}
      ref={frameRef}
      className={[
        "app-sidebar-frame",
        header && headerPlacement === "row" ? "app-sidebar-frame-with-header" : undefined,
        header && headerPlacement === "overlay" ? "app-sidebar-frame-with-overlay-header" : undefined,
        sidebarState.collapsed ? "sidebar-collapsed" : undefined,
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
      <div className="app-sidebar-pane">{sidebar}</div>
      {!sidebarState.collapsed ? (
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
            title="Drag to resize. Drag to the left edge to hide."
            tabIndex={0}
          />
        </>
      ) : (
        <button
          aria-label="Show sidebar"
          className="app-sidebar-expand"
          onClick={sidebarState.expand}
          title="Show sidebar"
          type="button"
        >
          <ChevronRight aria-hidden="true" size={14} />
        </button>
      )}
      {children}
    </main>
  );
}

function useAppSidebarState(frameRef: RefObject<HTMLElement | null>) {
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
    const width = clampDragWidth(drag.startWidth + event.clientX - drag.startX);
    drag.liveWidth = width;
    applySidebarWidth(frameRef.current, width);
  };
  const finishResize = (event: PointerEvent<HTMLElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    event.currentTarget.releasePointerCapture(event.pointerId);
    dragRef.current = undefined;
    setLayoutResizing(frameRef.current, false);
    if (drag.liveWidth <= COLLAPSE_THRESHOLD) {
      update({ collapsed: true, width: drag.startWidth });
      return;
    }
    update({
      collapsed: false,
      width: clampWidth(drag.liveWidth),
    });
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
    if (event.key === "Enter" || event.key === "Home") {
      event.preventDefault();
      update({ ...stateRef.current, collapsed: true });
      return;
    }
    const delta = event.key === "ArrowLeft" ? -16 : event.key === "ArrowRight" ? 16 : undefined;
    const absoluteWidth = event.key === "End" ? MAX_WIDTH : undefined;
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
    expand: () => update({ ...stateRef.current, collapsed: false }),
    finishResize,
    resize,
    resizeWithKeyboard,
  };
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
