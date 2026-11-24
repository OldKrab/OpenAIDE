import type { DeviceCollectionSnapshot } from "@openaide/app-server-client";
import { useState, type ReactNode } from "react";
import type { ComputersCapability } from "../services/computers";
import { UseAnotherComputerDialog } from "./settings/devicePairingDialogs";

/**
 * "Use another computer", the computer switcher's last item (ADR-0081), opened where
 * the switcher is. A shell that takes a code in place gets the same dialog as Settings;
 * a shell with its own pairing screen opens that screen; otherwise Settings › Remote
 * opens, where the Computers list ends with the same action.
 */
export function useAnotherComputerDialog({
  capability,
  device,
  devices,
  openSettings,
}: {
  capability: ComputersCapability | undefined;
  device: "computer" | "phone";
  devices: DeviceCollectionSnapshot | undefined;
  openSettings: () => void;
}): { open: () => void; dialog: ReactNode } {
  const [open, setOpen] = useState(false);
  const pair = capability?.pair;
  return {
    open: () => {
      if (pair) setOpen(true);
      // Settings shows why the shell's own screen did not open, and offers it again.
      else if (capability?.openPairing) void capability.openPairing().catch(openSettings);
      else openSettings();
    },
    dialog: open && pair ? (
      <UseAnotherComputerDialog
        device={device}
        devices={devices}
        onClose={() => setOpen(false)}
        pair={pair}
      />
    ) : null,
  };
}
