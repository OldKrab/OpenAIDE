import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import { ChevronDown, FolderPlus, Minus, Plus, Settings, Sparkles, Square, X } from "lucide-react";
import type { DesktopWindowCapability } from "../services/frontendShell";

export type DesktopTitleBarCommands = {
  addProject?: () => void;
  newTask: () => void;
  openSettings: () => void;
};

/** Renders product content inside shell-owned native window chrome. */
export function DesktopTitleBar({
  children,
  commands,
  window: desktopWindow,
}: {
  children?: ReactNode;
  commands: DesktopTitleBarCommands;
  window: DesktopWindowCapability;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const windowsChrome = desktopWindow.platform === "windows";

  useEffect(() => {
    if (!menuOpen) return undefined;
    const closeOutside = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setMenuOpen(false);
    };
    window.addEventListener("pointerdown", closeOutside);
    return () => window.removeEventListener("pointerdown", closeOutside);
  }, [menuOpen]);

  const run = (command: () => void) => {
    setMenuOpen(false);
    command();
  };
  const handleMenuKey = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    setMenuOpen(false);
  };
  const startDragging = (event: MouseEvent<HTMLDivElement>) => {
    if (event.button !== 0 || event.detail !== 1 || isInteractiveTitleBarTarget(event.target)) return;
    event.preventDefault();
    void desktopWindow.startDragging();
  };
  const toggleMaximize = (event: MouseEvent<HTMLDivElement>) => {
    if (event.button !== 0 || isInteractiveTitleBarTarget(event.target)) return;
    event.preventDefault();
    void desktopWindow.toggleMaximize();
  };
  // Both shells explicitly initiate native dragging. On macOS this prevents
  // AppKit from zooming before the Desktop host can pre-layout WKWebView.
  const dragRegionProps = { onDoubleClick: toggleMaximize, onMouseDown: startDragging };

  return (
    <header
      aria-label="Desktop window controls"
      className={`desktop-title-bar desktop-title-bar-${desktopWindow.platform}`}
    >
      <div className="desktop-title-bar-sidebar" {...dragRegionProps}>
        {windowsChrome ? (
          <div onKeyDown={handleMenuKey} ref={menuRef}>
            <button
              aria-expanded={menuOpen}
              aria-haspopup="menu"
              className="desktop-app-command"
              onClick={() => setMenuOpen((open) => !open)}
              type="button"
            >
              <span><Sparkles aria-hidden="true" size={14} /></span>
              <strong>OpenAIDE</strong>
              <ChevronDown aria-hidden="true" size={12} />
            </button>
            {menuOpen ? (
              <div className="desktop-app-menu" role="menu" aria-label="OpenAIDE actions">
                <CommandItem icon={<Plus size={15} />} label="New task" shortcut="Ctrl+N" onClick={() => run(commands.newTask)} />
                {commands.addProject ? <CommandItem icon={<FolderPlus size={15} />} label="Add project folder…" shortcut="Ctrl+O" onClick={() => run(commands.addProject!)} /> : null}
                <CommandItem icon={<Settings size={15} />} label="Settings" shortcut="Ctrl+," onClick={() => run(commands.openSettings)} />
                <hr />
                <CommandItem icon={<X size={15} />} label="Quit" onClick={() => void desktopWindow.close()} />
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
      <div className="desktop-title-bar-content" {...dragRegionProps}>
        {children}
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

function isInteractiveTitleBarTarget(target: EventTarget) {
  return Boolean((target as Element).closest?.("button, a, input, textarea, select, [role='button'], [role='menu']"));
}

function CommandItem({
  icon,
  label,
  onClick,
  shortcut,
}: {
  icon: ReactNode;
  label: string;
  onClick: () => void;
  shortcut?: string;
}) {
  return (
    <button onClick={onClick} role="menuitem" type="button">
      {icon}
      <span><strong>{label}</strong>{shortcut ? <small>{shortcut}</small> : null}</span>
    </button>
  );
}
