/** Live layout resize updates CSS vars without React. */

export function setLayoutResizing(root: ParentNode | null | undefined, resizing: boolean) {
  if (typeof HTMLElement === "undefined" || !(root instanceof HTMLElement)) return;
  if (resizing) root.dataset.resizing = "true";
  else delete root.dataset.resizing;
}

export function applySidebarWidth(frame: HTMLElement | null | undefined, width: number) {
  frame?.style?.setProperty?.("--app-sidebar-width", `${width}px`);
}

/** Split ratio lives on the Task stack so Plan drawer offset can share it with File Viewer. */
export function applyTaskPanelRatio(root: ParentNode | null | undefined, ratio: number) {
  if (typeof HTMLElement === "undefined" || !(root instanceof HTMLElement)) return;
  root.style.setProperty("--task-panel-ratio", String(ratio));
}
