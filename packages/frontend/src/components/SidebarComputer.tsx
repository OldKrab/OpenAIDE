import { useSyncExternalStore } from "react";
import { MonitorSmartphone } from "lucide-react";
import type { SettingsTabId } from "@openaide/app-shell-contracts";
import type { FrontendShell } from "../services/frontendShell";
import type { BackendConnectionState } from "./appControllerBackendLifecycle";

export type ComputerLink = "connected" | "connecting" | "reconnecting" | "notResponding" | "notConnected";

/** The other computer this shell works on, and the Settings tab that changes it. */
export type ComputerInUse = { name: string; settingsTab: SettingsTabId };

const linkLabels: Record<ComputerLink, string> = {
  connected: "Connected",
  connecting: "Connecting…",
  reconnecting: "Reconnecting…",
  notResponding: "Not responding",
  notConnected: "Not connected",
};

const noSubscription = () => () => {};
const noSnapshot = () => undefined;

/**
 * Names the other computer only while one is in use; working on this device
 * needs no indicator. The phone's choice changes live, the Desktop's is fixed
 * for the shell's run.
 */
export function useComputerInUse(shell: FrontendShell | undefined): ComputerInUse | undefined {
  const phone = shell?.connectionSettings;
  const snapshot = useSyncExternalStore(
    phone?.subscribe ?? noSubscription,
    phone?.snapshot ?? noSnapshot,
    phone?.snapshot ?? noSnapshot,
  );
  if (snapshot?.remote) {
    return { name: snapshot.computer || "Another computer", settingsTab: "remote" };
  }
  const desktop = shell?.remoteComputers?.current;
  return desktop ? { name: desktop, settingsTab: "remote" } : undefined;
}

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

/**
 * Heads the navigation with the computer its projects and tasks live on. The
 * dot alone says the link is healthy; words appear only when it is not.
 */
export function SidebarComputer({ link, name, onOpen }: {
  link: ComputerLink;
  name: string;
  onOpen: () => void;
}) {
  const label = linkLabels[link];
  return (
    <button
      aria-label={`Working on ${name}. ${label}. Change computer`}
      className="sidebar-computer"
      data-link={link}
      onClick={onOpen}
      title={`Working on ${name} · ${label}`}
      type="button"
    >
      <MonitorSmartphone size={15} />
      <span className="sidebar-computer-name">{name}</span>
      <span aria-hidden="true" className="sidebar-computer-dot" />
      {link === "connected" ? null : <span className="sidebar-computer-link">{label}</span>}
    </button>
  );
}
