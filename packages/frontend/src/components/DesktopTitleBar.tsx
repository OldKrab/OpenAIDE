import { type MouseEvent } from "react";
import { Minus, Square, X } from "lucide-react";
import type { DesktopWindowCapability } from "../services/frontendShell";

/** Renders product content inside shell-owned native window chrome. */
export function DesktopTitleBar({
  integrated = false,
  window: desktopWindow,
}: {
  integrated?: boolean;
  window: DesktopWindowCapability;
}) {
  const windowsChrome = desktopWindow.platform === "windows";
  const dragRegionProps = desktopDragRegionProps(desktopWindow);

  return (
    <header
      aria-label="Desktop window controls"
      className={[
        "desktop-title-bar",
        `desktop-title-bar-${desktopWindow.platform}`,
        integrated ? "desktop-title-bar-integrated" : undefined,
      ].filter(Boolean).join(" ")}
    >
      <div className="desktop-title-bar-sidebar" {...dragRegionProps} />
      <div className="desktop-title-bar-content" {...dragRegionProps}>
        {windowsChrome && !integrated ? <span className="desktop-title-bar-label">OpenAIDE</span> : null}
      </div>
      {windowsChrome ? (
        <div className="desktop-caption-buttons" aria-label="Window controls">
          <button aria-label="Minimize" onClick={() => void desktopWindow.minimize()} type="button"><Minus size={14} /></button>
          <button aria-label="Maximize or restore" onClick={() => void desktopWindow.toggleMaximize()} type="button"><Square size={11} /></button>
          <button aria-label="Close" onClick={() => void desktopWindow.close()} type="button"><X size={15} /></button>
        </div>
      ) : <div className="desktop-title-bar-native-end" {...dragRegionProps} />}
    </header>
  );
}

/** Makes product header whitespace behave like native Desktop window chrome. */
export function desktopDragRegionProps(desktopWindow: DesktopWindowCapability) {
  const startDragging = (event: MouseEvent<HTMLElement>) => {
    if (event.button !== 0 || event.detail !== 1 || isInteractiveTitleBarTarget(event.target)) return;
    event.preventDefault();
    void desktopWindow.startDragging();
  };
  const toggleMaximize = (event: MouseEvent<HTMLElement>) => {
    if (event.button !== 0 || isInteractiveTitleBarTarget(event.target)) return;
    event.preventDefault();
    void desktopWindow.toggleMaximize();
  };
  // Both shells explicitly initiate native dragging. On macOS this prevents
  // AppKit from zooming before the Desktop host can pre-layout WKWebView.
  return { onDoubleClick: toggleMaximize, onMouseDown: startDragging };
}

function isInteractiveTitleBarTarget(target: EventTarget) {
  return Boolean((target as Element).closest?.("button, a, input, textarea, select, [role='button'], [role='menu']"));
}
