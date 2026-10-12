import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";
import type { AuthTerminalSession } from "../../services/authTerminals";
import { frontendShell } from "../../services/frontendShell";
import { SettingsButton } from "./settingsKit";

/** A real terminal, not a provider-specific parser: escape sequences, prompts and input stay
 * Agent-owned. Links require an explicit click and never invoke host commands or clipboard APIs. */
export function AgentAuthTerminal({ session, onCancel }: {
  session: AuthTerminalSession;
  onCancel?: () => void;
}) {
  const element = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!element.current) return;
    const colors = getComputedStyle(element.current);
    const terminal = new Terminal({
      cols: session.cols, rows: session.rows, fontSize: 14, scrollback: 500,
      screenReaderMode: true, cursorBlink: false, allowProposedApi: false,
      theme: { background: colors.backgroundColor, foreground: colors.color },
      linkHandler: { activate: (_event, uri) => openTerminalLink(uri) },
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.loadAddon(new WebLinksAddon((_event, uri) => openTerminalLink(uri)));
    terminal.open(element.current);
    const detach = session.attach((bytes) => terminal.write(bytes));
    const input = terminal.onData((data) => session.send(data));
    const resize = () => {
      fit.fit();
      session.cols = Math.max(2, Math.min(500, terminal.cols));
      session.rows = Math.max(2, Math.min(200, terminal.rows));
    };
    const observer = new ResizeObserver(resize);
    observer.observe(element.current);
    resize();
    terminal.focus();
    return () => { observer.disconnect(); input.dispose(); detach(); terminal.dispose(); };
  }, [session]);
  return (
    <div className="settings-card settings-step">
      <header>
        <span className="settings-step-copy">
          <strong>Sign in using the agent’s terminal</strong>
          <p>Follow the instructions below. Sign-in completes when the agent exits successfully.</p>
        </span>
        <SettingsButton onClick={() => { session.close(); onCancel?.(); }} variant="quiet">Cancel sign-in</SettingsButton>
      </header>
      <div ref={element} className="settings-terminal-screen" aria-label="Agent sign-in terminal" />
    </div>
  );
}

function openTerminalLink(uri: string) {
  try {
    const url = new URL(uri);
    if (url.protocol === "https:") frontendShell().recovery.openExternal(url.href);
  } catch { /* Untrusted terminal text is not a URL. */ }
}
