import type { DeviceCollectionSnapshot, RemoteDeviceSummary } from "@openaide/app-server-client";
import { Laptop, Monitor, Plus, QrCode, Smartphone, Wifi } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { RemoteDeviceIntents } from "../../intents/remoteDeviceIntents";
import { computerName, currentComputer, useComputers, type ComputersCapability, type ComputersSnapshot, type ShellComputer } from "../../services/computers";
import { currentFrontendShell } from "../../services/frontendShell";
import { PopupDialog } from "../Popup";
import { InviteDialog, JoinDialog } from "./devicePairingDialogs";
import type { PhoneConnection } from "./phoneConnection";
import { settingsInfo } from "./settingsInfo";
import { errorSentence, SettingsButton, SettingsChoice, SettingsConfirm, SettingsInlineError, SettingsNotice } from "./settingsKit";
import type { SettingsGroup, SettingsRow, SettingsSection } from "./settingsModel";

export type RemoteSectionProps = {
  deviceIntents?: RemoteDeviceIntents;
  devices?: DeviceCollectionSnapshot;
  phone?: PhoneConnection;
};

type Dialog =
  | { kind: "pair" }
  | { kind: "forget"; computer: ShellComputer }
  | { kind: "invite" }
  | { kind: "join" }
  | { kind: "remove"; device: RemoteDeviceSummary };

/**
 * Both directions of remote use on one page: which computer this shell is
 * connected to, and which devices may connect to that computer.
 */
export function useRemoteSection({ deviceIntents, devices, phone }: RemoteSectionProps): SettingsSection | undefined {
  const capability = currentFrontendShell()?.computers;
  const computers = useSettingsComputers(capability);
  const [dialog, setDialog] = useState<Dialog>();
  const close = () => setDialog(undefined);
  if (!capability && !deviceIntents) return undefined;

  const inUse = currentComputer(computers.snapshot);
  const remoteName = inUse ? computerName(inUse) : undefined;
  const device = computers.snapshot?.device === "phone" || phone ? "phone" : "computer";
  const here = `this ${device}`;
  const groups: SettingsGroup[] = [];
  if (capability) {
    groups.push(computersGroup(computers, phone, () => {
      if (capability.pair) setDialog({ kind: "pair" });
      else void computers.openPairing();
    }, (computer) => setDialog({ kind: "forget", computer })));
  }
  if (deviceIntents) {
    // The App Server names itself; a browser tab on another machine is not "this computer".
    groups.push(devicesGroup(devices, devices?.serverName || remoteName || here, (next) => setDialog(next)));
  }

  return {
    id: "remote",
    label: "Remote",
    icon: Wifi,
    value: remoteName,
    groups,
    overlay: dialog ? (
      <>
        {dialog.kind === "pair" && capability?.pair ? <PairComputerDialog pair={capability.pair} onClose={close} /> : null}
        {dialog.kind === "forget" ? (
          <SettingsConfirm
            confirmLabel="Forget"
            detail={`This ${device} stops connecting to it.${dialog.computer.current ? ` Work moves to ${here}.` : ""} It stays in that computer's device list until you remove it there.`}
            label={`Forget ${computerName(dialog.computer)}?`}
            onClose={close}
            onConfirm={() => computers.forget(dialog.computer.id)}
            pendingLabel="Forgetting…"
            title={`Forget ${computerName(dialog.computer)}?`}
          />
        ) : null}
        {dialog.kind === "invite" && deviceIntents ? <InviteDialog devices={devices?.devices ?? []} intents={deviceIntents} onClose={close} /> : null}
        {dialog.kind === "join" && deviceIntents ? <JoinDialog intents={deviceIntents} onClose={close} /> : null}
        {dialog.kind === "remove" && deviceIntents ? (
          <SettingsConfirm
            confirmLabel="Remove device"
            detail="It is disconnected now and must be paired again to return."
            label="Remove device confirmation"
            onClose={close}
            onConfirm={() => deviceIntents.remove(dialog.device.deviceId)}
            pendingLabel="Removing…"
            title={`Remove ${dialog.device.name}?`}
          />
        ) : null}
      </>
    ) : undefined,
  };
}

type SettingsComputers = {
  snapshot: ComputersSnapshot | undefined;
  /** Id of the computer being switched to; "" stands for this device. */
  switching: string | undefined;
  failed: { id: string; message: string } | undefined;
  use(id?: string): Promise<void>;
  forget(id: string): Promise<void>;
  openPairing(): Promise<void>;
};

/**
 * The shell owns the computers and replaces the page to apply a switch, so a
 * settled `use` means the shell accepted it or refused with a sentence.
 */
function useSettingsComputers(capability: ComputersCapability | undefined): SettingsComputers {
  const snapshot = useComputers(capability);
  const [switching, setSwitching] = useState<string>();
  const [failed, setFailed] = useState<{ id: string; message: string }>();
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  return {
    snapshot,
    switching,
    failed,
    use: async (id) => {
      if (!capability || switching !== undefined) return;
      setFailed(undefined);
      setSwitching(id ?? "");
      try {
        await capability.use(id);
      } catch (cause) {
        if (mounted.current) {
          setSwitching(undefined);
          setFailed({ id: id ?? "", message: errorSentence(cause, "OpenAIDE could not switch. Try again.") });
        }
      }
    },
    forget: async (id) => {
      if (!capability) return;
      try {
        await capability.forget(id);
      } catch (cause) {
        throw new Error(errorSentence(cause, "OpenAIDE could not forget that computer. Try again."));
      }
    },
    openPairing: async () => {
      try {
        await capability?.openPairing?.();
      } catch (cause) {
        if (mounted.current) setFailed({ id: "pair", message: errorSentence(cause, "Pairing could not open. Try again.") });
      }
    },
  };
}

function computersGroup(
  computers: SettingsComputers,
  phone: PhoneConnection | undefined,
  onPair: () => void,
  onForget: (computer: ShellComputer) => void,
): SettingsGroup {
  const snapshot = computers.snapshot;
  const list = snapshot?.computers ?? [];
  const remote = list.some((computer) => computer.current);
  const busy = computers.switching !== undefined || Boolean(phone?.busy);
  const onPhone = snapshot?.device === "phone";
  const here = onPhone ? "This phone" : "This computer";
  const rows: SettingsRow[] = [];
  if (snapshot) {
    rows.push({
      id: onPhone ? "this-phone" : "this-computer",
      label: here,
      description: onPhone ? "Runs in Termux" : "Projects, tools and Agents here",
      keywords: onPhone ? "phone local termux connect switch" : "computer remote connect switch pc local",
      leading: onPhone ? <Smartphone aria-hidden="true" size={18} strokeWidth={1.6} /> : <Monitor aria-hidden="true" size={18} strokeWidth={1.6} />,
      selected: !remote,
      error: computers.failed?.id === "" ? computers.failed.message : undefined,
      control: (
        <SettingsChoice
          active={!remote}
          busy={computers.switching === ""}
          busyLabel="Switching…"
          disabled={busy}
          label={here}
          onUse={() => void computers.use()}
        />
      ),
    });
  }
  for (const computer of list) {
    const name = computerName(computer);
    rows.push({
      id: `computer-${computer.id}`,
      label: name,
      description: "Paired computer",
      keywords: "computer remote connect switch pc",
      leading: <Laptop aria-hidden="true" size={18} strokeWidth={1.6} />,
      selected: computer.current,
      error: computers.failed?.id === computer.id ? computers.failed.message : undefined,
      control: (
        <>
          <SettingsButton ariaLabel={`Forget ${name}`} disabled={busy} onClick={() => onForget(computer)} variant="quiet">Forget</SettingsButton>
          <SettingsChoice
            active={computer.current}
            busy={computers.switching === computer.id}
            busyLabel="Switching…"
            disabled={busy}
            label={name}
            onUse={() => void computers.use(computer.id)}
          />
        </>
      ),
    });
  }
  const state = phone?.state;
  const pairFailed = computers.failed?.id === "pair" ? computers.failed.message : undefined;
  const loadError = snapshot?.error;
  return {
    id: "connected-to",
    title: "Connected to",
    info: settingsInfo.connectedTo,
    actions: <SettingsButton disabled={busy} icon={<Plus aria-hidden="true" size={14} />} onClick={onPair}>Pair a computer</SettingsButton>,
    notice: (
      <>
        {loadError ? <SettingsInlineError>{loadError}</SettingsInlineError> : null}
        {pairFailed ? <SettingsInlineError>{pairFailed}</SettingsInlineError> : null}
        {state?.remote && state.notice ? <SettingsNotice>{state.notice}</SettingsNotice> : null}
        {phone?.error ? <SettingsInlineError>{phone.error}</SettingsInlineError> : null}
      </>
    ),
    empty: loadError || phone?.error ? undefined : "Loading…",
    rows,
  };
}

function devicesGroup(
  devices: DeviceCollectionSnapshot | undefined,
  owner: string,
  open: (dialog: Dialog) => void,
): SettingsGroup {
  const paired = devices?.devices ?? [];
  return {
    id: "devices",
    title: `Devices paired with ${owner}`,
    info: settingsInfo.devices,
    actions: devices ? (
      <>
        <SettingsButton onClick={() => open({ kind: "join" })}>Enter code</SettingsButton>
        <SettingsButton icon={<QrCode aria-hidden="true" size={14} />} onClick={() => open({ kind: "invite" })} variant="primary">Show code</SettingsButton>
      </>
    ) : undefined,
    notice: devices?.remoteAccess === "failed" ? (
      <SettingsInlineError>Remote access could not start, so paired devices cannot connect. Check the network and restart OpenAIDE.</SettingsInlineError>
    ) : devices?.remoteAccess === "starting" ? <SettingsNotice>Starting remote access…</SettingsNotice> : undefined,
    empty: !devices ? "Loading…" : devices.remoteAccess === "off" ? "None. Pairing a device turns on remote access." : "None.",
    rows: paired.map((device): SettingsRow => {
      const connection = device.connection ?? undefined;
      const added = `Paired ${formatMoment(device.addedAtMs)}${device.addedBy ? ` from ${device.addedBy}` : ""}`;
      const model = device.model && device.model !== device.name ? `${device.model} · ` : "";
      return {
        id: device.deviceId,
        label: device.name,
        description: `${model}${connection ? added : device.lastSeenAtMs ? `Last seen ${formatMoment(device.lastSeenAtMs)}` : "Not connected yet"}`,
        keywords: `phone device pair ${device.model ?? ""}`,
        leading: <Smartphone aria-hidden="true" size={18} strokeWidth={1.6} />,
        status: connection
          ? { tone: "ok", text: `Connected · ${connection.path === "direct" ? "direct" : "relay"}` }
          : { tone: "off", text: "Offline" },
        control: (
          <SettingsButton ariaLabel={`Remove ${device.name}`} danger onClick={() => open({ kind: "remove", device })} variant="quiet">Remove</SettingsButton>
        ),
      };
    }),
  };
}

/** Spends a code another computer shows; success moves the shell to that computer's work. */
function PairComputerDialog({ pair: spend, onClose }: { pair: (code: string) => Promise<void>; onClose: () => void }) {
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const pair = async () => {
    setBusy(true);
    setError(undefined);
    try {
      await spend(code);
      onClose();
    } catch (cause) {
      setError(errorSentence(cause, "Pairing failed. Try again."));
      setBusy(false);
    }
  };
  return (
    <PopupDialog className="settings-confirm" label="Pair with another computer" onOpenChange={(open) => { if (!open && !busy) onClose(); }} open>
      <strong>Pair a computer</strong>
      <p>On the other computer, open Settings, Remote, choose Show code, then copy it here. OpenAIDE restarts and opens that computer's projects.</p>
      <textarea
        aria-label="Pairing code"
        autoFocus
        className="settings-code-input"
        onChange={(event) => setCode(event.currentTarget.value)}
        placeholder="OAI1…"
        rows={3}
        spellCheck={false}
        value={code}
      />
      {error ? <SettingsInlineError>{error}</SettingsInlineError> : null}
      <footer>
        <SettingsButton disabled={busy} onClick={onClose}>Cancel</SettingsButton>
        <SettingsButton busy={busy} disabled={!code.trim()} onClick={() => void pair()} variant="primary">{busy ? "Pairing…" : "Pair"}</SettingsButton>
      </footer>
    </PopupDialog>
  );
}

function formatMoment(timestampMs: number) {
  return new Date(timestampMs).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}
