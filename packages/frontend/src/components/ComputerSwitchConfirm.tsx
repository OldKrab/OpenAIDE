import { computerName, type ComputersSnapshot } from "../services/computers";
import { useComputerSwitch, type ComputerSwitch } from "../services/computerSwitch";
import { SettingsConfirm } from "./settings/settingsKit";

/**
 * Asks before a switch drops input it cannot keep (ADR-0070). Rendered once beside the
 * App's surfaces, so the Task navigation and Settings share it.
 */
export function ComputerSwitchConfirm({ snapshot, switcher }: {
  snapshot: ComputersSnapshot | undefined;
  switcher: ComputerSwitch;
}) {
  const { confirming, draftsAtRisk } = useComputerSwitch(switcher);
  if (confirming === undefined) return null;
  const computer = snapshot?.computers.find((candidate) => candidate.id === confirming);
  const target = computer ? computerName(computer) : snapshot?.device === "phone" ? "this phone" : "this computer";
  return (
    <SettingsConfirm
      confirmLabel="Discard and switch"
      detail={draftsAtRisk
        ? "Your unsent message, attachments, and uploads on this page will be discarded."
        : "Your attachments, uploads, and any message still sending on this page will be discarded. Message drafts will be here when you come back."}
      label={`Switch to ${target}?`}
      onClose={() => switcher.decline()}
      onConfirm={() => { void switcher.confirm(); }}
      title={`Switch to ${target}?`}
    />
  );
}
