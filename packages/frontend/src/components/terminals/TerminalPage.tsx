import { useEffect, useRef, useState } from "react";
import { ArrowLeft, Keyboard, Plus, SquareTerminal, WifiOff } from "lucide-react";
import type { TerminalAttach, TerminalSummary, TerminalTarget } from "@openaide/app-server-client";
import type { TerminalIntents } from "../../intents/terminalIntents";
import type { DesktopWindowCapability } from "../../services/frontendShell";
import type { TerminalStreamState } from "../../services/terminalSockets";
import { desktopDragRegionProps } from "../DesktopTitleBar";
import { StateDot, TerminalCloseButton, TerminalKeyRow, terminalTitles, useVisibleHeight } from "./TerminalParts";
import { TerminalScreen } from "./TerminalScreen";
import type { TerminalPageTarget } from "./terminalSurface";

export type TerminalPageProps = {
  target: TerminalPageTarget;
  projectLabel: string;
  computerName?: string;
  /** Every Terminal of the App Server; absent until the subscription's baseline arrives. */
  terminals?: TerminalSummary[];
  intents: TerminalIntents;
  socketUrl(): string;
  /** The phone layout: chips, a key row, and a page sized to the visible viewport. */
  phone: boolean;
  desktopWindow?: DesktopWindowCapability;
  onBack(): void;
};

/**
 * The Terminal page (ADR-0071): a Project's shells on the App Server's computer, as tabs. The
 * App Server owns the list, order and status; this page keeps only which tab is shown.
 */
export default function TerminalPage(props: TerminalPageProps) {
  const { target, terminals, intents, phone } = props;
  const projectTerminals = (terminals ?? []).filter((terminal) => terminal.projectId === target.projectId);
  const [activeId, setActiveId] = useState<string>();
  const [opening, setOpening] = useState(false);
  const [openError, setOpenError] = useState<string>();
  const [streamState, setStreamState] = useState<TerminalStreamState>("connecting");
  const [ctrl, setCtrl] = useState(false);
  // An open's token is spent by the first viewer; later viewers ask for their own.
  const firstAttach = useRef(new Map<string, TerminalAttach>());
  const send = useRef<((data: string) => void) | undefined>(undefined);
  const visibleHeight = useVisibleHeight(phone);
  const active = projectTerminals.find((terminal) => terminal.terminalId === activeId) ?? projectTerminals.at(-1);
  const titles = terminalTitles(projectTerminals);
  const titleOf = (terminal: TerminalSummary) => titles.get(terminal.terminalId) ?? "";

  const open = async () => {
    if (opening) return;
    setOpening(true);
    setOpenError(undefined);
    const terminalTarget: TerminalTarget = target.taskId
      ? { kind: "task", taskId: target.taskId as never }
      : { kind: "project", projectId: target.projectId as never };
    try {
      const result = await intents.open(terminalTarget, { cols: 80, rows: 24 });
      firstAttach.current.set(result.terminal.terminalId, result.attach);
      setActiveId(result.terminal.terminalId);
    } catch {
      setOpenError("The terminal could not start. Try again when connected.");
    } finally {
      setOpening(false);
    }
  };

  // Entering the page with nothing to show starts the shell the user asked for.
  const openedOnEntry = useRef(false);
  useEffect(() => {
    if (openedOnEntry.current || terminals === undefined) return;
    openedOnEntry.current = true;
    if (projectTerminals.length === 0) void open();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once, after the first baseline
  }, [terminals === undefined]);

  const close = async (terminal: TerminalSummary, force: boolean) => {
    const outcome = await intents.close(terminal.terminalId, force);
    if (outcome !== "busy" && terminal.terminalId === active?.terminalId) {
      const index = projectTerminals.indexOf(terminal);
      const rest = projectTerminals.filter((candidate) => candidate !== terminal);
      setActiveId(rest[Math.min(index, rest.length - 1)]?.terminalId);
    }
    return outcome;
  };

  const screen = active ? (
    <div className="terminal-screen-wrap">
      <TerminalScreen
        attach={intents.attach}
        ctrl={ctrl}
        fontSize={phone ? 12 : 13}
        initialAttach={firstAttach.current.get(active.terminalId)}
        inputRef={send}
        key={active.terminalId}
        label={titleOf(active)}
        onCtrlUsed={() => setCtrl(false)}
        onStreamState={(state) => {
          if (state === "live") firstAttach.current.delete(active.terminalId);
          setStreamState(state);
        }}
        socketUrl={props.socketUrl}
        terminalId={active.terminalId}
      />
      {streamState === "reconnecting" && active.status !== "exited" ? (
        <div className="terminal-offline" role="status">
          <WifiOff size={14} />
          Reconnecting{props.computerName ? ` to ${props.computerName}` : ""}… <span>Your shells keep running.</span>
        </div>
      ) : null}
      {active.status === "exited" ? (
        <div className="terminal-exited" role="status">
          Shell exited{active.exitCode != null ? ` with code ${active.exitCode}` : ""}
        </div>
      ) : null}
    </div>
  ) : (
    <div className="terminal-empty">
      <SquareTerminal size={28} />
      <p>{terminals === undefined || opening ? "Starting a terminal…" : "No open terminals"}</p>
      {openError ? <p className="terminal-error" role="alert">{openError}</p> : null}
      {terminals !== undefined && !opening ? (
        <button className="terminal-button" onClick={() => void open()} type="button">
          <Plus size={14} />New terminal in {props.projectLabel}
        </button>
      ) : null}
    </div>
  );
  const newButton = (
    <button aria-label="New terminal" className="terminal-icon" disabled={opening} onClick={() => void open()} title="New terminal" type="button">
      <Plus size={15} />
    </button>
  );

  if (phone) {
    return (
      <section aria-label="Terminal" className="terminal-page terminal-page-phone" style={visibleHeight ? { height: visibleHeight } : undefined}>
        <header className="terminal-phone-bar">
          <button aria-label="Back to conversation" className="terminal-icon" onClick={props.onBack} type="button"><ArrowLeft size={18} /></button>
          <span className="terminal-phone-heading">
            <strong>Terminal</strong>
            {props.computerName ? <small>{props.computerName}</small> : null}
          </span>
          <button
            aria-label="Toggle keyboard"
            className="terminal-icon"
            onClick={() => {
              const field = document.querySelector<HTMLTextAreaElement>(".terminal-page .xterm-helper-textarea");
              if (document.activeElement === field) field?.blur();
              else field?.focus();
            }}
            onPointerDown={(event) => event.preventDefault()}
            type="button"
          >
            <Keyboard size={17} />
          </button>
        </header>
        <div aria-label="Terminals" className="terminal-chips" role="tablist">
          {projectTerminals.map((terminal) => (
            <button
              aria-selected={terminal === active}
              key={terminal.terminalId}
              onClick={() => setActiveId(terminal.terminalId)}
              role="tab"
              type="button"
            >
              <StateDot status={terminal.status} />{titleOf(terminal)}
            </button>
          ))}
          {newButton}
          {active ? <TerminalCloseButton onClose={(force) => close(active, force)} terminal={active} title={titleOf(active)} /> : null}
        </div>
        {openError && active ? <p className="terminal-error terminal-error-strip" role="alert">{openError}</p> : null}
        {screen}
        {active && active.status !== "exited" ? <TerminalKeyRow ctrl={ctrl} onCtrl={() => setCtrl(!ctrl)} send={(data) => send.current?.(data)} /> : null}
      </section>
    );
  }

  return (
    <section aria-label="Terminal" className="terminal-page">
      <header className="terminal-page-header" {...(props.desktopWindow ? desktopDragRegionProps(props.desktopWindow) : {})}>
        <button className="terminal-button terminal-quiet" onClick={props.onBack} type="button"><ArrowLeft size={15} />Conversation</button>
        <div className="terminal-page-title">
          <strong>Terminal</strong>
          <span>{props.projectLabel}{props.computerName ? ` · ${props.computerName}` : ""}</span>
        </div>
        {active ? <span className="terminal-cwd" title={active.folder}>{"\u200e"}{active.folder}</span> : null}
      </header>
      <div aria-label="Terminals" className="terminal-tabs" role="tablist">
        {projectTerminals.map((terminal) => {
          const title = titleOf(terminal);
          return (
            <div
              aria-selected={terminal === active}
              className="terminal-tab"
              key={terminal.terminalId}
              onClick={() => setActiveId(terminal.terminalId)}
              onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") setActiveId(terminal.terminalId); }}
              role="tab"
              tabIndex={0}
            >
              <StateDot status={terminal.status} />
              <span className="terminal-tab-title">{title}</span>
              <TerminalCloseButton onClose={(force) => close(terminal, force)} terminal={terminal} title={title} />
            </div>
          );
        })}
        {newButton}
        {openError && active ? <span className="terminal-error" role="alert">{openError}</span> : null}
      </div>
      {screen}
    </section>
  );
}
