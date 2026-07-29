import { ChevronRight } from "lucide-react";
import {
  useRef,
  useState,
  type ComponentPropsWithoutRef,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
} from "react";

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
  sidebar: ReactNode;
};

/**
 * Owns the stable application sidebar frame. Sidebar contents can change by
 * surface without changing the user's width or collapse preference.
 */
export function AppSidebarFrame({
  children,
  className,
  sidebar,
  style,
  ...rootProps
}: AppSidebarFrameProps) {
  const sidebarState = useAppSidebarState();
  return (
    <main
      {...rootProps}
      className={[
        "app-sidebar-frame",
        sidebarState.collapsed ? "sidebar-collapsed" : undefined,
        className,
      ].filter(Boolean).join(" ")}
      style={{
        ...style,
        "--app-sidebar-width": `${sidebarState.width}px`,
      } as React.CSSProperties}
    >
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

function useAppSidebarState() {
  const [state, setState] = useState<SidebarFrameState>(readState);
  const stateRef = useRef(state);
  const dragRef = useRef<{
    pointerId: number;
    startWidth: number;
    startX: number;
  } | undefined>(undefined);

  const update = (next: SidebarFrameState, persist = true) => {
    stateRef.current = next;
    setState(next);
    if (persist) writeState(next);
  };
  const beginResize = (event: PointerEvent<HTMLElement>) => {
    if (event.button !== 0) return;
    dragRef.current = {
      pointerId: event.pointerId,
      startWidth: stateRef.current.width,
      startX: event.clientX,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const resize = (event: PointerEvent<HTMLElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    update({
      ...stateRef.current,
      width: clampDragWidth(drag.startWidth + event.clientX - drag.startX),
    }, false);
  };
  const finishResize = (event: PointerEvent<HTMLElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    event.currentTarget.releasePointerCapture(event.pointerId);
    dragRef.current = undefined;
    if (stateRef.current.width <= COLLAPSE_THRESHOLD) {
      update({ collapsed: true, width: drag.startWidth });
      return;
    }
    update({
      collapsed: false,
      width: clampWidth(stateRef.current.width),
    });
  };
  const cancelResize = (event: PointerEvent<HTMLElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    event.currentTarget.releasePointerCapture(event.pointerId);
    dragRef.current = undefined;
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
