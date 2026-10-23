import { lazy, Suspense } from "react";
import type { TerminalSummary } from "@openaide/app-server-client";
import type { TerminalIntents } from "../../intents/terminalIntents";
import type { DesktopWindowCapability, TerminalSocketCapability } from "../../services/frontendShell";
import type { TerminalPageTarget } from "./terminalSurface";

// xterm is large and only this page and Agent sign-in need it.
const TerminalPage = lazy(() => import("./TerminalPage"));

/** Renders the Terminal page in the workbench's main surface while one is open. */
export function TerminalPageHost({ target, terminals, intents, socket, projects, computerName, phone, desktopWindow, onBack }: {
  target?: TerminalPageTarget;
  terminals?: TerminalSummary[];
  intents?: TerminalIntents;
  socket?: TerminalSocketCapability;
  projects: Array<{ projectId: string; label: string }>;
  computerName?: string;
  phone: boolean;
  desktopWindow?: DesktopWindowCapability;
  onBack(): void;
}) {
  if (!target || !intents || !socket) return null;
  const projectLabel = projects.find((project) => project.projectId === target.projectId)?.label ?? "Project";
  return (
    <Suspense fallback={<section aria-busy="true" aria-label="Terminal" className="terminal-page" />}>
      <TerminalPage
        computerName={computerName}
        desktopWindow={desktopWindow}
        intents={intents}
        key={target.projectId}
        onBack={onBack}
        phone={phone}
        projectLabel={projectLabel}
        socketUrl={() => socket.socketUrl()}
        target={target}
        terminals={terminals}
      />
    </Suspense>
  );
}
