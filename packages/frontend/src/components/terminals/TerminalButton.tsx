import { createPortal } from "react-dom";
import { SquareTerminal } from "lucide-react";
import { currentFrontendShell } from "../../services/frontendShell";
import { MobileTaskMoreRow, useMobileTaskMore } from "../MobileTaskMore";
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
  /** The phone's More menu, where the entry is a row with a count badge. */
  target?: HTMLElement | null;
}) {
  const surface = useTerminalSurface();
  const { close } = useMobileTaskMore();
  const nativeTerminal = surface ? undefined : currentFrontendShell()?.nativeTerminal;
  if (!projectId || !(surface || (nativeTerminal && workspaceRoot))) return null;
  const count = surface ? openTerminalCount(surface.terminals, projectId) : 0;
  const label = count > 0 ? `Terminal, ${count} open` : "Terminal";
  const open = () => {
    if (surface) surface.openPage({ projectId, taskId });
    else if (workspaceRoot) nativeTerminal?.open(workspaceRoot);
  };
  const badge = count > 0 ? <span className="terminal-count">{count}</span> : null;
  if (target) {
    return createPortal(
      <MobileTaskMoreRow
        aria-label={label}
        end={badge}
        icon={<SquareTerminal aria-hidden="true" />}
        label="Terminal"
        onClick={() => {
          close();
          open();
        }}
        slot="terminal"
      />,
      target,
    );
  }
  return (
    <button aria-label={label} className="project-files-entry terminal-entry" onClick={open} title="Terminal" type="button">
      <SquareTerminal aria-hidden="true" size={14} />
      Terminal
      {badge}
    </button>
  );
}
