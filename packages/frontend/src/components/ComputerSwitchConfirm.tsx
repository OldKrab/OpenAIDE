import { computerName, type ComputersSnapshot } from "../services/computers";
import { useComputerSwitch, type ComputerSwitch } from "../services/computerSwitch";
import { SettingsConfirm } from "./settings/settingsKit";

/**
 * Asks before a switch drops unsent input (ADR-0070). Rendered once beside the
 * App's surfaces, so the Task navigation and Settings share it.
 */
export function ComputerSwitchConfirm({ snapshot, switcher }: {
  snapshot: ComputersSnapshot | undefined;
  switcher: ComputerSwitch;
}) {
  const { confirming } = useComputerSwitch(switcher);
  if (confirming === undefined) return null;
  const computer = snapshot?.computers.find((candidate) => candidate.id === confirming);
  const target = computer ? computerName(computer) : snapshot?.device === "phone" ? "this phone" : "this computer";
  return (
    <SettingsConfirm
      confirmLabel="Discard and switch"
      detail="Your unsent message, attachments, and uploads on this page will be discarded."
      label={`Switch to ${target}?`}
      onClose={() => switcher.decline()}
      onConfirm={() => { void switcher.confirm(); }}
      title={`Switch to ${target}?`}
    />
  );
}
