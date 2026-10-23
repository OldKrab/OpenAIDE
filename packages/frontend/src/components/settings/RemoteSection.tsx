import type { DeviceCollectionSnapshot, RemoteDeviceSummary } from "@openaide/app-server-client";
import { Laptop, Monitor, Plus, QrCode, Server, Smartphone, Wifi } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { RemoteDeviceIntents } from "../../intents/remoteDeviceIntents";
import { computerName, currentComputer, useComputers, type ComputersCapability, type ComputersSnapshot, type ShellComputer, type SshComputerStatus } from "../../services/computers";
import { computerSwitch, useComputerSwitch } from "../../services/computerSwitch";
import type { PhoneSshComputer } from "../../services/connectionSettings";
import { currentFrontendShell } from "../../services/frontendShell";
import { PopupDialog } from "../Popup";
import { InviteDialog, JoinDialog } from "./devicePairingDialogs";
import type { PhoneConnection } from "./phoneConnection";
import { settingsInfo } from "./settingsInfo";
import { AddSshComputerDialog, UpdateSshComputerDialog, useSshStatus } from "./sshComputers";
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
  | { kind: "add-ssh" }
  | { kind: "update-ssh" }
  | { kind: "stop-ssh"; computer: ShellComputer }
  | { kind: "stop-phone-ssh"; computer: PhoneSshComputer }
  | { kind: "forget-phone-ssh"; computer: PhoneSshComputer }
  | { kind: "update-phone-ssh" }
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
  const ssh = capability?.ssh;
  const sshStatus = useSshStatus(ssh);
  const [sshNotice, setSshNotice] = useState<string>();
  const [dialog, setDialog] = useState<Dialog>();
  const close = () => setDialog(undefined);
  if (!capability && !deviceIntents) return undefined;

  const inUse = currentComputer(computers.snapshot);
  const phoneSsh = phone?.state?.sshComputers?.find((computer) => computer.current);
  const remoteName = inUse ? computerName(inUse) : phoneSsh?.name;
  const device = computers.snapshot?.device === "phone" || phone ? "phone" : "computer";
  const here = `this ${device}`;
  const groups: SettingsGroup[] = [];
  if (capability) {
    groups.push(computersGroup(computers, phone, {
      onPair: () => {
        if (capability.pair) setDialog({ kind: "pair" });
        else void computers.openPairing();
      },
      open: setDialog,
      canAddSsh: Boolean(ssh),
      sshStatus,
      sshNotice,
    }));
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
            detail={dialog.computer.ssh
              ? `This ${device} forgets it.${dialog.computer.current ? ` Work moves to ${here}.` : ""} OpenAIDE keeps running there until you stop it or no app uses it.`
              : `This ${device} stops connecting to it.${dialog.computer.current ? ` Work moves to ${here}.` : ""} It stays in that computer's device list until you remove it there.`}
            label={`Forget ${computerName(dialog.computer)}?`}
            onClose={close}
            onConfirm={() => computers.forget(dialog.computer.id)}
            pendingLabel="Forgetting…"
            title={`Forget ${computerName(dialog.computer)}?`}
          />
        ) : null}
        {dialog.kind === "add-ssh" && ssh ? <AddSshComputerDialog onClose={close} ssh={ssh} /> : null}
        {dialog.kind === "update-ssh" && ssh && sshStatus ? (
          <UpdateSshComputerDialog
            computer={remoteName ?? "The SSH computer"}
            impact={deviceIntents?.replacementImpact}
            onClose={close}
            onUpdate={() => ssh.update()}
            status={sshStatus}
          />
        ) : null}
        {dialog.kind === "stop-ssh" && ssh ? (
          <SettingsConfirm
            confirmLabel="Stop"
            detail={`OpenAIDE on ${dialog.computer.name} stops once no app is connected to it, ending work in progress there.${dialog.computer.current ? " This app stays connected until you switch away." : ""}`}
            label={`Stop OpenAIDE on ${dialog.computer.name}?`}
            onClose={close}
            onConfirm={async () => {
              const outcome = await ssh.stop(dialog.computer.id);
              setSshNotice(outcome === "notRunning"
                ? `OpenAIDE was not kept running on ${dialog.computer.name}.`
                : outcome === "stillRunning"
                  ? `OpenAIDE on ${dialog.computer.name} did not stop yet. Try again in a moment.`
                  : `OpenAIDE on ${dialog.computer.name} stops once no app is connected to it.`);
            }}
            pendingLabel="Stopping…"
            title={`Stop OpenAIDE on ${dialog.computer.name}?`}
          />
        ) : null}
        {dialog.kind === "stop-phone-ssh" && phone ? (
          <SettingsConfirm
            confirmLabel="Stop"
            detail={`OpenAIDE on ${dialog.computer.name} stops once no app is connected to it, ending work in progress there.${dialog.computer.current ? " This phone stays connected until you switch away." : ""}`}
            label={`Stop OpenAIDE on ${dialog.computer.name}?`}
            onClose={close}
            onConfirm={() => phone.execute({ action: "stop_ssh", id: dialog.computer.id })}
            pendingLabel="Stopping…"
            title={`Stop OpenAIDE on ${dialog.computer.name}?`}
          />
        ) : null}
        {dialog.kind === "forget-phone-ssh" && phone ? (
          <SettingsConfirm
            confirmLabel="Forget"
            detail={`This phone forgets it and its host key.${dialog.computer.current ? " Work moves to this phone." : ""} OpenAIDE keeps running there until you stop it or no app uses it.`}
            label={`Forget ${dialog.computer.name}?`}
            onClose={close}
            onConfirm={() => phone.execute({ action: "forget_ssh", id: dialog.computer.id })}
            pendingLabel="Forgetting…"
            title={`Forget ${dialog.computer.name}?`}
          />
        ) : null}
        {dialog.kind === "update-phone-ssh" && phone?.state?.ssh ? (
          <UpdateSshComputerDialog
            computer={phoneSsh?.name || "The SSH computer"}
            impact={deviceIntents?.replacementImpact}
            onClose={close}
            onUpdate={() => phone.execute({ action: "update_ssh" })}
            status={phone.state.ssh}
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
  /** The pending switch can still be ended. */
  cancellable: boolean;
  failed: { id: string; message: string } | undefined;
  use(id?: string): void;
  cancel(): void;
  forget(id: string): Promise<void>;
  openPairing(): Promise<void>;
};

/**
 * The shell owns the computers and applies a switch; the switch itself is the
 * one the Task navigation shows too, so both agree on what is connecting.
 */
function useSettingsComputers(capability: ComputersCapability | undefined): SettingsComputers {
  const snapshot = useComputers(capability);
  const switcher = capability ? computerSwitch(capability) : undefined;
  const switched = useComputerSwitch(switcher);
  const [pairFailed, setPairFailed] = useState<string>();
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  return {
    snapshot,
    switching: switched.pending,
    cancellable: switched.cancellable,
    failed: pairFailed ? { id: "pair", message: pairFailed } : switched.failed,
    use: (id) => {
      setPairFailed(undefined);
      void switcher?.request(id);
    },
    cancel: () => switcher?.cancel(),
    forget: async (id) => {
      if (!capability) return;
      try {
        await capability.forget(id);
      } catch (cause) {
        throw new Error(errorSentence(cause, "OpenAIDE could not forget that computer. Try again."));
      }
    },
    openPairing: async () => {
      setPairFailed(undefined);
      try {
        await capability?.openPairing?.();
      } catch (cause) {
        if (mounted.current) setPairFailed(errorSentence(cause, "Pairing could not open. Try again."));
      }
    },
  };
}

type ComputersView = {
  onPair(): void;
  open(dialog: Dialog): void;
  /** The shell can start the App Server on SSH Computers. */
  canAddSsh: boolean;
  /** The shell's report on the SSH Computer in use. */
  sshStatus: SshComputerStatus | undefined;
  /** The outcome of the last Stop. */
  sshNotice: string | undefined;
};

function computersGroup(computers: SettingsComputers, phone: PhoneConnection | undefined, view: ComputersView): SettingsGroup {
  const snapshot = computers.snapshot;
  const list = snapshot?.computers ?? [];
  // TODO(ssh-computers): the phone keeps its SSH Computers in its Settings state and
  // switches to them outside the computer switch (ADR-0070); list them with the
  // other computers, so the Task navigation offers them too, once the phone does.
  const phoneSsh = phone?.state?.sshComputers ?? [];
  const remote = list.some((computer) => computer.current) || phoneSsh.some((computer) => computer.current);
  const busy = computers.switching !== undefined || Boolean(phone?.busy);
  const onPhone = snapshot?.device === "phone";
  const here = onPhone ? "This phone" : "This computer";
  const rows: SettingsRow[] = [];
  const cancelButton = (id: string) => computers.switching === id && computers.cancellable
    ? <SettingsButton onClick={() => computers.cancel()} variant="quiet">Cancel</SettingsButton>
    : undefined;
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
        <>
          {cancelButton("")}
          <SettingsChoice
            active={!remote}
            busy={computers.switching === ""}
            busyLabel="Switching…"
            disabled={busy}
            label={here}
            onUse={() => computers.use()}
          />
        </>
      ),
    });
  }
  for (const computer of list) {
    const name = computerName(computer);
    rows.push({
      id: `computer-${computer.id}`,
      label: name,
      description: computer.ssh ? `SSH · ${computer.ssh}` : "Paired computer",
      keywords: computer.ssh ? "computer remote connect switch ssh server linux" : "computer remote connect switch pc",
      leading: computer.ssh
        ? <Server aria-hidden="true" size={18} strokeWidth={1.6} />
        : <Laptop aria-hidden="true" size={18} strokeWidth={1.6} />,
      selected: computer.current,
      error: computers.failed?.id === computer.id ? computers.failed.message : undefined,
      control: (
        <>
          {cancelButton(computer.id) ?? (
            <>
              {computer.ssh ? (
                <SettingsButton ariaLabel={`Stop OpenAIDE on ${name}`} disabled={busy} onClick={() => view.open({ kind: "stop-ssh", computer })} variant="quiet">Stop</SettingsButton>
              ) : null}
              <SettingsButton ariaLabel={`Forget ${name}`} disabled={busy} onClick={() => view.open({ kind: "forget", computer })} variant="quiet">Forget</SettingsButton>
            </>
          )}
          <SettingsChoice
            active={computer.current}
            busy={computers.switching === computer.id}
            busyLabel="Switching…"
            disabled={busy}
            label={name}
            onUse={() => computers.use(computer.id)}
          />
        </>
      ),
    });
  }
  for (const computer of phoneSsh) {
    rows.push({
      id: `ssh-${computer.id}`,
      label: computer.name,
      description: `SSH · ${computer.destination}`,
      keywords: "computer remote connect switch ssh server linux",
      leading: <Server aria-hidden="true" size={18} strokeWidth={1.6} />,
      selected: computer.current,
      control: (
        <>
          <SettingsButton ariaLabel={`Stop OpenAIDE on ${computer.name}`} disabled={busy} onClick={() => view.open({ kind: "stop-phone-ssh", computer })} variant="quiet">Stop</SettingsButton>
          <SettingsButton ariaLabel={`Forget ${computer.name}`} disabled={busy} onClick={() => view.open({ kind: "forget-phone-ssh", computer })} variant="quiet">Forget</SettingsButton>
          {computer.current
            ? <SettingsChoice active label={computer.name} onUse={() => undefined} />
            : <SettingsButton ariaLabel={`Use ${computer.name}`} disabled={busy} onClick={() => void phone?.execute({ action: "use_ssh", id: computer.id })}>Use</SettingsButton>}
        </>
      ),
    });
  }
  const state = phone?.state;
  const pairFailed = computers.failed?.id === "pair" ? computers.failed.message : undefined;
  const loadError = snapshot?.error;
  const current = list.find((computer) => computer.current && computer.ssh) ?? phoneSsh.find((computer) => computer.current);
  const sshStatus = current ? (phone ? state?.ssh ?? undefined : view.sshStatus) : undefined;
  const canAddSsh = phone ? Boolean(state?.sshAvailable) : view.canAddSsh;
  return {
    id: "connected-to",
    title: "Connected to",
    info: settingsInfo.connectedTo,
    actions: (
      <>
        {canAddSsh ? (
          <SettingsButton
            disabled={busy}
            icon={<Plus aria-hidden="true" size={14} />}
            onClick={() => phone ? void phone.execute({ action: "ssh_add" }) : view.open({ kind: "add-ssh" })}
          >
            Add SSH computer
          </SettingsButton>
        ) : null}
        <SettingsButton disabled={busy} icon={<Plus aria-hidden="true" size={14} />} onClick={view.onPair}>Pair a computer</SettingsButton>
      </>
    ),
    notice: (
      <>
        {loadError ? <SettingsInlineError>{loadError}</SettingsInlineError> : null}
        {pairFailed ? <SettingsInlineError>{pairFailed}</SettingsInlineError> : null}
        {state?.remote && state.notice ? <SettingsNotice>{state.notice}</SettingsNotice> : null}
        {state?.sshNotice ? <SettingsNotice>{state.sshNotice}</SettingsNotice> : null}
        {view.sshNotice ? <SettingsNotice>{view.sshNotice}</SettingsNotice> : null}
        {sshStatus?.updateAvailable && current ? (
          <SettingsNotice>
            {current.name} runs OpenAIDE {sshStatus.runningVersion}, older than this app.{" "}
            <SettingsButton disabled={busy} onClick={() => view.open({ kind: phone ? "update-phone-ssh" : "update-ssh" })} variant="quiet">Update…</SettingsButton>
          </SettingsNotice>
        ) : null}
        {sshStatus?.suggestLinger && current ? (
          <SettingsNotice>
            OpenAIDE on {current.name} stops when you log out there. Run <code>loginctl enable-linger</code> on {current.name} to keep it running.
          </SettingsNotice>
        ) : null}
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
      <p>On the other computer, open Settings, Remote, choose Show code, then copy it here. OpenAIDE then opens that computer's projects.</p>
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
