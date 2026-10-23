import { createPortal } from "react-dom";
import { SquareTerminal } from "lucide-react";
import { currentFrontendShell } from "../../services/frontendShell";
import { openTerminalCount, useTerminalSurface } from "./terminalSurface";

/**
 * The Task header's way into its Project's Terminals; new ones start in the Task Workspace.
 * A shell with its own terminal (VS Code) opens that at the Task Workspace instead; any other
 * shell without the in-app Terminal page renders nothing.
 */
export function TerminalButton({ projectId, taskId, target, workspaceRoot }: {
  projectId?: string;
  taskId: string;
  workspaceRoot?: string;
  /** The phone header slot, where the entry is an icon with a count badge. */
  target?: HTMLElement | null;
}) {
  const surface = useTerminalSurface();
  const nativeTerminal = surface ? undefined : currentFrontendShell()?.nativeTerminal;
  if (!projectId || !(surface || (nativeTerminal && workspaceRoot))) return null;
  const count = surface ? openTerminalCount(surface.terminals, projectId) : 0;
  const label = count > 0 ? `Terminal, ${count} open` : "Terminal";
  const button = (
    <button
      aria-label={label}
      className="project-files-entry terminal-entry"
      onClick={() => {
        if (surface) surface.openPage({ projectId, taskId });
        else if (workspaceRoot) nativeTerminal?.open(workspaceRoot);
      }}
      title="Terminal"
      type="button"
    >
      <SquareTerminal aria-hidden="true" size={target ? 18 : 14} />
      {target ? null : "Terminal"}
      {count > 0 ? <span className="terminal-count">{count}</span> : null}
    </button>
  );
  return target ? createPortal(button, target) : button;
}
