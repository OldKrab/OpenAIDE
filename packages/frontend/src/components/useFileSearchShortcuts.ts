import { useEffect } from "react";
import type { FileSearchMode } from "./ProjectFilesPalette";

/** Ctrl/Cmd+P finds a file and Ctrl/Cmd+Shift+F searches contents wherever Project files is available. */
export function useFileSearchShortcuts(enabled: boolean, open: (mode: FileSearchMode) => void) {
  useEffect(() => {
    if (!enabled) return;
    const key = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
      const letter = event.key.toLowerCase();
      const mode = letter === "p" && !event.shiftKey ? "files" : letter === "f" && event.shiftKey ? "text" : undefined;
      if (!mode) return;
      event.preventDefault();
      open(mode);
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [enabled, open]);
}
