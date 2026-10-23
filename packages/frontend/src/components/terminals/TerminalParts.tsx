import { useEffect, useRef, useState, type ReactNode } from "react";
import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp, X } from "lucide-react";
import type { TerminalSummary } from "@openaide/app-server-client";

/** The running command while one runs, else the name of the shell's current folder. */
export function terminalTitle(terminal: TerminalSummary) {
  if (terminal.status === "busy" && terminal.command) return terminal.command;
  return terminal.folder.split(/[\\/]/).filter(Boolean).at(-1) ?? terminal.folder;
}

/**
 * Titles for a row of tabs: a title that repeats gains a number in tab order ("atlas",
 * "atlas 2"), so two shells in one folder stay distinguishable.
 */
export function terminalTitles(terminals: readonly TerminalSummary[]) {
  const seen = new Map<string, number>();
  const titles = new Map<string, string>();
  for (const terminal of terminals) {
    const title = terminalTitle(terminal);
    const count = (seen.get(title) ?? 0) + 1;
    seen.set(title, count);
    titles.set(terminal.terminalId, count === 1 ? title : `${title} ${count}`);
  }
  return titles;
}

export function StateDot({ status }: { status: TerminalSummary["status"] }) {
  const label = status === "busy" ? "Running" : status === "exited" ? "Exited" : "Idle";
  return <span className={`terminal-dot terminal-dot-${status}`} role="img" aria-label={label} />;
}

/**
 * Closing a shell that runs a program asks first, anchored to its tab. App Server decides what
 * counts as running, so a close that it refuses as busy also lands here.
 */
export function TerminalCloseButton({ terminal, title, onClose }: {
  terminal: TerminalSummary;
  title: string;
  onClose(force: boolean): Promise<"closed" | "busy" | "gone">;
}) {
  const [confirming, setConfirming] = useState(false);
  const [closing, setClosing] = useState(false);
  const panel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!confirming) return;
    panel.current?.querySelector<HTMLButtonElement>("button")?.focus();
    const dismiss = (event: PointerEvent) => {
      if (!panel.current?.contains(event.target as Node)) setConfirming(false);
    };
    document.addEventListener("pointerdown", dismiss);
    return () => document.removeEventListener("pointerdown", dismiss);
  }, [confirming]);
  const close = async (force: boolean) => {
    setClosing(true);
    try {
      if ((await onClose(force)) === "busy") setConfirming(true);
      else setConfirming(false);
    } finally {
      setClosing(false);
    }
  };
  return (
    <span className="terminal-close-wrap">
      <button
        aria-label={`Close ${title}`}
        className="terminal-tab-close"
        disabled={closing}
        onClick={(event) => {
          event.stopPropagation();
          if (terminal.status === "busy") setConfirming(true);
          else void close(false);
        }}
        type="button"
      >
        <X size={13} />
      </button>
      {confirming ? (
        <div
          aria-label="Close running terminal"
          className="terminal-confirm"
          onClick={(event) => event.stopPropagation()}
          onKeyDown={(event) => { if (event.key === "Escape") setConfirming(false); }}
          ref={panel}
          role="alertdialog"
        >
          <strong>Stop {terminal.command ? <code>{terminal.command}</code> : "the running program"}?</strong>
          <p>Closing this terminal ends the command.</p>
          <div>
            <button className="terminal-button" onClick={() => setConfirming(false)} type="button">Cancel</button>
            <button className="terminal-button terminal-danger" disabled={closing} onClick={() => void close(true)} type="button">
              Close terminal
            </button>
          </div>
        </div>
      ) : null}
    </span>
  );
}

const KEYS: Array<{ label: ReactNode; name: string; send: string }> = [
  { label: "esc", name: "Escape", send: "\x1b" },
  { label: "tab", name: "Tab", send: "\t" },
  { label: "^C", name: "Interrupt", send: "\x03" },
  { label: <ArrowUp size={15} />, name: "Up", send: "\x1b[A" },
  { label: <ArrowDown size={15} />, name: "Down", send: "\x1b[B" },
  { label: <ArrowLeft size={15} />, name: "Left", send: "\x1b[D" },
  { label: <ArrowRight size={15} />, name: "Right", send: "\x1b[C" },
  { label: "|", name: "Pipe", send: "|" },
  { label: "~", name: "Tilde", send: "~" },
  { label: "/", name: "Slash", send: "/" },
  { label: "-", name: "Dash", send: "-" },
];

/**
 * Keys a phone keyboard lacks. Pressing one never takes focus, so the soft keyboard stays up.
 */
export function TerminalKeyRow({ ctrl, onCtrl, send }: { ctrl: boolean; onCtrl(): void; send(data: string): void }) {
  return (
    <div
      aria-label="Terminal keys"
      className="terminal-keys"
      onMouseDown={(event) => event.preventDefault()}
      onPointerDown={(event) => event.preventDefault()}
      role="toolbar"
    >
      <button aria-pressed={ctrl} className="terminal-key terminal-key-ctrl" onClick={onCtrl} type="button">ctrl</button>
      {KEYS.map((key) => (
        <button aria-label={key.name} className="terminal-key" key={key.name} onClick={() => send(key.send)} type="button">
          {key.label}
        </button>
      ))}
    </div>
  );
}

/** Sizes the page to the visible viewport so the key row sits on top of the soft keyboard. */
export function useVisibleHeight(enabled: boolean) {
  const [height, setHeight] = useState<number>();
  useEffect(() => {
    const viewport = window.visualViewport;
    if (!enabled || !viewport) {
      setHeight(undefined);
      return;
    }
    const sync = () => {
      setHeight(viewport.height);
      window.scrollTo(0, 0);
    };
    sync();
    viewport.addEventListener("resize", sync);
    return () => viewport.removeEventListener("resize", sync);
  }, [enabled]);
  return height;
}
