import { useEffect, useState, type KeyboardEvent } from "react";
import { Check, Monitor, MonitorSmartphone } from "lucide-react";
import { computerName, currentComputer, type ComputersSnapshot } from "../services/computers";
import { useComputerSwitch, type ComputerSwitch } from "../services/computerSwitch";
import type { BackendConnectionState } from "./appControllerBackendLifecycle";
import { PopupMenu } from "./Popup";

export type ComputerLink = "connected" | "connecting" | "reconnecting" | "notResponding" | "notConnected";

const linkLabels: Record<ComputerLink, string> = {
  connected: "Connected",
  connecting: "Connecting…",
  reconnecting: "Reconnecting…",
  notResponding: "Not responding",
  notConnected: "Not connected",
};

export function computerLink(state: BackendConnectionState): ComputerLink {
  switch (state.status) {
    case "ready":
      return state.unresponsive ? "notResponding" : "connected";
    case "connecting":
      return "connecting";
    case "reconnecting":
      return "reconnecting";
    case "unavailable":
      return "notConnected";
  }
}

/** "" stands for this device in the pending and failed switch. */
type Row = { id: string; label: string; current: boolean };

/**
 * Switches the computer in use in place (ADR-0070). It appears only once
 * another computer exists, so a user who never pairs sees nothing new. The
 * trigger names the other computer in use, with the dot alone saying the link
 * is healthy; this device needs only the icon.
 */
export function ComputerSwitcher({ link, onManage, openRequest = 0, snapshot, switcher }: {
  link: ComputerLink;
  onManage: () => void;
  /** Increments when a shell command asks to open the list. */
  openRequest?: number;
  snapshot: ComputersSnapshot | undefined;
  /** The shell's switch, shared with Settings so both show the same attempt. */
  switcher: ComputerSwitch;
}) {
  const [open, setOpen] = useState(false);
  const { pending } = useComputerSwitch(switcher);
  useEffect(() => {
    if (openRequest > 0) setOpen(true);
  }, [openRequest]);
  if (!snapshot?.computers.length) return null;

  const inUse = currentComputer(snapshot);
  const here = snapshot.device === "phone" ? "This phone" : "This computer";
  const rows: Row[] = [
    { id: "", label: here, current: !inUse },
    ...snapshot.computers.map((computer) => ({ id: computer.id, label: computerName(computer), current: computer.current })),
  ];
  const label = linkLabels[link];
  const triggerLabel = inUse
    ? `Working on ${computerName(inUse)}. ${label}. Change computer`
    : `Working on ${here.toLowerCase()}. Change computer`;

  const choose = (row: Row) => {
    if (pending !== undefined) return;
    if (row.current) {
      setOpen(false);
      return;
    }
    // The wait and its outcome are a page (ComputerSwitchPage), not this popup.
    setOpen(false);
    void switcher.request(row.id || undefined);
  };
  const pickByNumber = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.altKey || event.ctrlKey || event.metaKey || !/^[1-9]$/.test(event.key)) return;
    const row = rows[Number(event.key) - 1];
    if (!row) return;
    event.preventDefault();
    choose(row);
  };

  return (
    <PopupMenu
      className="computer-switcher-menu"
      label="Computers"
      onOpenChange={setOpen}
      open={open}
      placement="top-end"
      trigger={(triggerProps) => (
        <button
          {...triggerProps}
          aria-busy={pending !== undefined ? true : undefined}
          aria-label={triggerLabel}
          className="computer-switcher-trigger"
          data-link={inUse ? link : undefined}
          title={inUse ? `Working on ${computerName(inUse)} · ${label}` : `Working on ${here.toLowerCase()}`}
          type="button"
        >
          <MonitorSmartphone aria-hidden="true" size={15} />
          {inUse ? <span className="computer-switcher-name">{computerName(inUse)}</span> : null}
          {/* The dot's colour carries the link state so the name keeps the room; the words are
              in the button's label and tooltip, and the page itself says when nothing answers. */}
          {inUse ? <span aria-hidden="true" className="computer-switcher-dot" /> : null}
        </button>
      )}
    >
      <div className="computer-switcher-list" onKeyDown={pickByNumber}>
        {rows.map((row, index) => (
          <button
            aria-checked={row.current}
            aria-disabled={pending !== undefined ? true : undefined}
            className="computer-switcher-option"
            key={row.id || "this-device"}
            onClick={() => choose(row)}
            role="menuitemradio"
            type="button"
          >
            {index < 9 ? <kbd aria-hidden="true">{index + 1}</kbd> : null}
            {row.id ? <MonitorSmartphone aria-hidden="true" size={14} /> : <Monitor aria-hidden="true" size={14} />}
            <span className="computer-switcher-option-name">{row.label}</span>
            {pending === row.id ? <small>Connecting…</small> : null}
            {row.current ? <Check aria-hidden="true" className="computer-switcher-check" size={14} /> : null}
          </button>
        ))}
        <button
          className="computer-switcher-manage"
          onClick={() => {
            setOpen(false);
            onManage();
          }}
          role="menuitem"
          type="button"
        >
          Pair or manage computers…
        </button>
      </div>
    </PopupMenu>
  );
}

