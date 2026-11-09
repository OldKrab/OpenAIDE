import type { Terminal } from "@xterm/xterm";

/** Ctrl+V pastes in a browser terminal on Windows and Linux, as it does in desktop terminals
 * such as Windows Terminal and VS Code's. xterm.js would otherwise send it to the program as
 * the ^V control character, and Agents read that as "paste an image" from a clipboard on the
 * App Server's computer, so nothing appears. Leaving the key to the browser fires its native
 * paste, which xterm.js inserts as typed text, so no clipboard permission is needed. macOS
 * pastes with Cmd+V and keeps Ctrl+V as a control key. */
export function leavesPasteToBrowser(
  event: Pick<KeyboardEvent, "key" | "ctrlKey" | "shiftKey" | "altKey" | "metaKey">,
  platform: string,
): boolean {
  if (/Mac|iPhone|iPad/.test(platform)) return false;
  return event.ctrlKey && !event.shiftKey && !event.altKey && !event.metaKey
    && event.key.toLowerCase() === "v";
}

/** Installs the Ctrl+V paste convention on `terminal`; see `leavesPasteToBrowser`. */
export function pasteWithCtrlV(terminal: Terminal, platform = navigator.platform ?? "") {
  // False tells xterm.js to skip the key without cancelling it, so the browser still pastes.
  terminal.attachCustomKeyEventHandler((event) => !leavesPasteToBrowser(event, platform));
}
