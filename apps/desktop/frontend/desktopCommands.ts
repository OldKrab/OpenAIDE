export type DesktopCommand = "check-for-updates" | "new-task" | "open-project" | "quit" | "settings";
export type DesktopSurfaceCommand = Exclude<DesktopCommand, "quit">;

type KeyboardCommandEvent = Pick<
  KeyboardEvent,
  "altKey" | "ctrlKey" | "key" | "metaKey" | "shiftKey"
>;

export function desktopCommandForKeyboardEvent(event: KeyboardCommandEvent): DesktopSurfaceCommand | undefined {
  if (!event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return undefined;
  if (event.key.toLowerCase() === "n") return "new-task";
  if (event.key.toLowerCase() === "o") return "open-project";
  if (event.key === ",") return "settings";
  return undefined;
}
