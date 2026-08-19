/** Live layout resize updates CSS vars without React; Chat wrap still follows the column. */

export function setLayoutResizing(root: ParentNode | null | undefined, resizing: boolean) {
  if (typeof HTMLElement === "undefined" || !(root instanceof HTMLElement)) return;
  if (resizing) root.dataset.resizing = "true";
  else delete root.dataset.resizing;
}

export function applySidebarWidth(frame: HTMLElement | null | undefined, width: number) {
  frame?.style?.setProperty?.("--app-sidebar-width", `${width}px`);
}
