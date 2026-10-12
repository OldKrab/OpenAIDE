import type { DeviceCollectionSnapshot, RemoteDeviceSummary } from "@openaide/app-server-client";
import { Laptop, Monitor, Plus, QrCode, Smartphone, Wifi } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { RemoteDeviceIntents } from "../../intents/remoteDeviceIntents";
import { currentFrontendShell, type RemoteComputer, type RemoteComputersCapability } from "../../services/frontendShell";
import type { ConnectionComputer } from "../../services/connectionSettings";
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
  | { kind: "forget"; computer: RemoteComputer }
  | { kind: "forget-phone"; computer: ConnectionComputer }
  | { kind: "invite" }
  | { kind: "join" }
  | { kind: "remove"; device: RemoteDeviceSummary };

/**
 * Both directions of remote use on one page: which computer this shell is
 * connected to, and which devices may connect to that computer.
 */
export function useRemoteSection({ deviceIntents, devices, phone }: RemoteSectionProps): SettingsSection | undefined {
  const capability = currentFrontendShell()?.remoteComputers;
  const desktop = useRemoteComputers(capability);
  const [dialog, setDialog] = useState<Dialog>();
  const close = () => setDialog(undefined);
  if (!capability && !phone && !deviceIntents) return undefined;

  const remoteName = capability?.current ?? (phone?.state?.remote ? phone.state.computer || "Remote computer" : undefined);
  const groups: SettingsGroup[] = [];
  if (capability) groups.push(desktopComputersGroup(desktop, () => setDialog({ kind: "pair" }), (computer) => setDialog({ kind: "forget", computer })));
  else if (phone) groups.push(phoneComputersGroup(phone, (computer) => setDialog({ kind: "forget-phone", computer })));
  if (deviceIntents) {
    const here = phone ? "this phone" : "this computer";
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
        {dialog.kind === "pair" && capability ? <PairComputerDialog capability={capability} onClose={close} /> : null}
        {dialog.kind === "forget" && capability ? (
          <SettingsConfirm
            confirmLabel="Forget"
            detail={`This computer stops connecting to it.${dialog.computer.current ? " OpenAIDE restarts and works on this computer." : ""} It stays in that computer's device list until you remove it there.`}
            label={`Forget ${dialog.computer.name}?`}
            onClose={close}
            onConfirm={() => desktop.forget(dialog.computer.id)}
            pendingLabel="Forgetting…"
            title={`Forget ${dialog.computer.name}?`}
          />
        ) : null}
        {dialog.kind === "forget-phone" && phone ? (
          <SettingsConfirm
            confirmLabel="Forget"
            detail={`This phone stops connecting to it.${dialog.computer.current ? " Work moves to this phone." : ""} Pair again to return.`}
            label={`Forget ${dialog.computer.name || "this computer"}?`}
            onClose={close}
            onConfirm={() => phone.execute({ action: "forget_computer", id: dialog.computer.id })}
            pendingLabel="Forgetting…"
            title={`Forget ${dialog.computer.name || "this computer"}?`}
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

type DesktopComputers = {
  computers: RemoteComputer[] | undefined;
  /** Id of the computer being switched to; "" stands for this computer. */
  switching: string | undefined;
  failed: { id: string; message: string } | undefined;
  loadError: string;
  use(id?: string): Promise<void>;
  forget(id: string): Promise<void>;
};

/**
 * The shell owns the paired computers and restarts to apply a switch, so a
 * settled promise means nothing changed or the shell refused with a sentence.
 */
function useRemoteComputers(capability: RemoteComputersCapability | undefined): DesktopComputers {
  const [computers, setComputers] = useState<RemoteComputer[]>();
  const [switching, setSwitching] = useState<string>();
  const [failed, setFailed] = useState<{ id: string; message: string }>();
  const [loadError, setLoadError] = useState("");
  const mounted = useRef(true);
  const refresh = async () => {
    if (!capability) return;
    try {
      const next = await capability.list();
      if (mounted.current) {
        setComputers(next);
        setLoadError("");
      }
    } catch {
      if (mounted.current) setLoadError("OpenAIDE could not read its paired computers. Reopen Settings to try again.");
    }
  };
  useEffect(() => {
    mounted.current = true;
    void refresh();
    return () => { mounted.current = false; };
  }, [capability]);
  return {
    computers,
    switching,
    failed,
    loadError,
    use: async (id) => {
      if (!capability || switching !== undefined) return;
      setFailed(undefined);
      setSwitching(id ?? "");
      try {
        await capability.use(id);
        await refresh();
      } catch (cause) {
        if (mounted.current) setFailed({ id: id ?? "", message: errorSentence(cause, "OpenAIDE could not switch. Try again.") });
      } finally {
        if (mounted.current) setSwitching(undefined);
      }
    },
    forget: async (id) => {
      if (!capability) return;
      try {
        await capability.forget(id);
      } catch (cause) {
        throw new Error(errorSentence(cause, "OpenAIDE could not forget that computer. Try again."));
      }
      await refresh();
    },
  };
}

function desktopComputersGroup(
  desktop: DesktopComputers,
  onPair: () => void,
  onForget: (computer: RemoteComputer) => void,
): SettingsGroup {
  const computers = desktop.computers ?? [];
  const remote = computers.some((computer) => computer.current);
  const busy = desktop.switching !== undefined;
  const rows: SettingsRow[] = [{
    id: "this-computer",
    label: "This computer",
    description: "Projects, tools and Agents here",
    keywords: "computer remote connect switch pc local",
    leading: <Monitor aria-hidden="true" size={18} strokeWidth={1.6} />,
    selected: !remote,
    error: desktop.failed?.id === "" ? desktop.failed.message : undefined,
    control: (
      <SettingsChoice
        active={!remote}
        busy={desktop.switching === ""}
        disabled={busy}
        label="This computer"
        onUse={() => void desktop.use()}
      />
    ),
  }];
  for (const computer of computers) {
    rows.push({
      id: `computer-${computer.id}`,
      label: computer.name,
      description: "Paired computer",
      keywords: "computer remote connect switch pc",
      leading: <Laptop aria-hidden="true" size={18} strokeWidth={1.6} />,
      selected: computer.current,
      error: desktop.failed?.id === computer.id ? desktop.failed.message : undefined,
      control: (
        <>
          <SettingsButton ariaLabel={`Forget ${computer.name}`} disabled={busy} onClick={() => onForget(computer)} variant="quiet">Forget</SettingsButton>
          <SettingsChoice
            active={computer.current}
            busy={desktop.switching === computer.id}
            disabled={busy}
            label={computer.name}
            onUse={() => void desktop.use(computer.id)}
          />
        </>
      ),
    });
  }
  return {
    id: "connected-to",
    title: "Connected to",
    info: settingsInfo.connectedTo,
    actions: <SettingsButton disabled={busy} icon={<Plus aria-hidden="true" size={14} />} onClick={onPair}>Pair a computer</SettingsButton>,
    notice: desktop.loadError ? <SettingsInlineError>{desktop.loadError}</SettingsInlineError> : undefined,
    rows,
  };
}

function phoneComputersGroup(phone: PhoneConnection, onForget: (computer: ConnectionComputer) => void): SettingsGroup {
  const state = phone.state;
  const rows: SettingsRow[] = [];
  if (state) {
    rows.push({
      id: "this-phone",
      label: "This phone",
      description: "Runs in Termux",
      keywords: "phone local termux connect switch",
      leading: <Smartphone aria-hidden="true" size={18} strokeWidth={1.6} />,
      selected: !state.remote,
      control: state.remote
        ? <SettingsButton ariaLabel="Use This phone" disabled={phone.busy} onClick={() => void phone.execute({ action: "local" })}>Use</SettingsButton>
        : <SettingsChoice active label="This phone" onUse={() => undefined} />,
    });
    for (const computer of state.computers ?? []) {
      const name = computer.name || "Remote computer";
      rows.push({
        id: `computer-${computer.id}`,
        label: name,
        description: "Paired computer",
        keywords: "computer remote connect switch pc",
        leading: <Laptop aria-hidden="true" size={18} strokeWidth={1.6} />,
        selected: computer.current,
        control: (
          <>
            <SettingsButton ariaLabel={`Forget ${computer.name || "this computer"}`} disabled={phone.busy} onClick={() => onForget(computer)} variant="quiet">Forget</SettingsButton>
            {computer.current
              ? <SettingsChoice active label={name} onUse={() => undefined} />
              : <SettingsButton ariaLabel={`Use ${name}`} disabled={phone.busy} onClick={() => void phone.execute({ action: "use_computer", id: computer.id })}>Use</SettingsButton>}
          </>
        ),
      });
    }
  }
  return {
    id: "connected-to",
    title: "Connected to",
    info: settingsInfo.connectedTo,
    actions: (
      <SettingsButton disabled={phone.busy} icon={<Plus aria-hidden="true" size={14} />} onClick={() => void phone.execute({ action: "pair_setup" })}>
        Pair a computer
      </SettingsButton>
    ),
    notice: (
      <>
        {state?.remote && state.notice ? <SettingsNotice>{state.notice}</SettingsNotice> : null}
        {phone.error ? <SettingsInlineError>{phone.error}</SettingsInlineError> : null}
      </>
    ),
    empty: phone.error ? undefined : "Loading…",
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

/** Spends a code another computer shows; success restarts the shell on that computer's work. */
function PairComputerDialog({ capability, onClose }: { capability: RemoteComputersCapability; onClose: () => void }) {
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const pair = async () => {
    setBusy(true);
    setError(undefined);
    try {
      await capability.pair(code);
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
