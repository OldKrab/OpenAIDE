import { useEffect, useRef, type MutableRefObject } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import type { TerminalAttach } from "@openaide/app-server-client";
import { connectTerminalStream, type TerminalStreamState } from "../../services/terminalSockets";

/** Ctrl with a letter or one of `@[\]^_` is that character's control code. */
export function withCtrl(data: string): string | undefined {
  if (data.length !== 1 || !/[a-z@[\]\\^_]/i.test(data)) return undefined;
  return String.fromCharCode(data.toUpperCase().charCodeAt(0) & 31);
}

/**
 * One Terminal's screen. Mounting attaches a viewer and unmounting detaches it; the shell keeps
 * running on the App Server, which replays its recent output to the next viewer.
 */
export function TerminalScreen({
  terminalId,
  label,
  initialAttach,
  attach,
  socketUrl,
  fontSize,
  ctrl,
  onCtrlUsed,
  inputRef,
  onStreamState,
}: {
  terminalId: string;
  label: string;
  initialAttach?: TerminalAttach;
  attach(terminalId: string): Promise<TerminalAttach>;
  socketUrl(): string;
  fontSize: number;
  /** The phone key row's sticky ctrl: the next typed character becomes its control code. */
  ctrl?: boolean;
  onCtrlUsed?(): void;
  /** Receives the input path so the key row types into the same stream. */
  inputRef?: MutableRefObject<((data: string) => void) | undefined>;
  onStreamState(state: TerminalStreamState): void;
}) {
  const element = useRef<HTMLDivElement>(null);
  // The stream outlives renders; these read the latest props without reconnecting.
  const latest = useRef({ ctrl, onCtrlUsed, attach, socketUrl, onStreamState, initialAttach });
  latest.current = { ctrl, onCtrlUsed, attach, socketUrl, onStreamState, initialAttach };

  useEffect(() => {
    const host = element.current;
    if (!host) return;
    const colors = getComputedStyle(host);
    const terminal = new Terminal({
      fontSize,
      fontFamily: colors.getPropertyValue("--oa-mono-font").trim() || "monospace",
      lineHeight: 1.25,
      scrollback: 10_000,
      cursorBlink: true,
      allowProposedApi: false,
      theme: {
        background: colors.backgroundColor,
        foreground: colors.color,
        cursor: colors.color,
        selectionBackground: "#4d9cff55",
      },
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(host);
    const fitSafely = () => {
      try { fit.fit(); } catch { /* A hidden screen has no size yet. */ }
    };
    fitSafely();
    const stream = connectTerminalStream({
      terminalId,
      initialAttach: latest.current.initialAttach,
      attach: () => latest.current.attach(terminalId),
      socketUrl: () => latest.current.socketUrl(),
      size: () => ({ cols: clamp(terminal.cols, 2, 500), rows: clamp(terminal.rows, 2, 200) }),
      onReset: () => terminal.reset(),
      onOutput: (bytes) => terminal.write(bytes),
      onState: (state) => latest.current.onStreamState(state),
    });
    const send = (data: string) => {
      if (latest.current.ctrl) {
        const control = withCtrl(data);
        if (control) {
          stream.write(control);
          latest.current.onCtrlUsed?.();
          return;
        }
      }
      stream.write(data);
    };
    if (inputRef) inputRef.current = send;
    const input = terminal.onData(send);
    const resized = terminal.onResize(({ cols, rows }) => stream.resize(clamp(cols, 2, 500), clamp(rows, 2, 200)));
    const observer = new ResizeObserver(fitSafely);
    observer.observe(host);
    terminal.focus();
    return () => {
      observer.disconnect();
      input.dispose();
      resized.dispose();
      if (inputRef) inputRef.current = undefined;
      stream.detach();
      terminal.dispose();
    };
  }, [fontSize, inputRef, terminalId]);

  return <div ref={element} className="terminal-screen" aria-label={`Terminal ${label}`} />;
}

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}
