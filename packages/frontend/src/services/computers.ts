import { useSyncExternalStore } from "react";

/** A computer other than this device whose App Server the shell can use (ADR-0063). */
export type ShellComputer = { id: string; name: string; current: boolean };

export type ComputersSnapshot = {
  /** Names the row for the device the shell runs on. */
  device: "computer" | "phone";
  /** Other computers in the order they were added; this device is in use when none is current. */
  computers: ShellComputer[];
  /** A sentence for the user when the shell could not read its list. */
  error?: string;
};

/**
 * What a switch request came to. `switched` means the shell is replacing the
 * page with the computer chosen; the other two leave the page as it is.
 */
export type ComputerSwitchResult = "switched" | "unchanged" | "cancelled";

/**
 * Shell-owned choice of whose App Server is in use (ADR-0070). The shell keeps
 * the list; the shared Frontend renders it and asks for a switch. The shell
 * connects the chosen computer first and replaces the page only once it
 * answered; a computer it could not reach rejects with a sentence for the user.
 */
export type ComputersCapability = {
  /** Undefined until the shell reports its first list. */
  snapshot(): ComputersSnapshot | undefined;
  subscribe(listener: () => void): () => void;
  /**
   * Uses another computer, or this device when no id is given. `operationId`
   * correlates the shell's diagnostics with the Frontend's.
   */
  use(id: string | undefined, operationId?: string): Promise<ComputerSwitchResult>;
  /** Ends a switch that is still connecting; absent where a switch cannot be cancelled. */
  cancel?(): Promise<void>;
  /** Forgetting the computer in use moves to this device first. */
  forget(id: string): Promise<void>;
  /** Spends the invite code another computer shows; absent where pairing has its own screen. */
  pair?(code: string): Promise<void>;
  /** Opens the shell's own pairing screen; absent where pairing takes a code in place. */
  openPairing?(): Promise<void>;
};

const noSubscription = () => () => undefined;
const noSnapshot = () => undefined;

export function useComputers(capability: ComputersCapability | undefined): ComputersSnapshot | undefined {
  return useSyncExternalStore(
    capability?.subscribe ?? noSubscription,
    capability?.snapshot ?? noSnapshot,
    capability?.snapshot ?? noSnapshot,
  );
}

/** The computer in use, or undefined while this device is. */
export function currentComputer(snapshot: ComputersSnapshot | undefined): ShellComputer | undefined {
  return snapshot?.computers.find((computer) => computer.current);
}

/** Paired computers may report no name; the list still needs a label. */
export function computerName(computer: ShellComputer): string {
  return computer.name || "Remote computer";
}
