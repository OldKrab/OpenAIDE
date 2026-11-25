import type { DeviceCollectionSnapshot, DevicesCreateInviteResult, RemoteDeviceSummary } from "@openaide/app-server-client";
import { ArrowRight, Copy, Monitor, MonitorSmartphone, Smartphone } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { encode } from "uqr";
import type { RemoteDeviceIntents } from "../../intents/remoteDeviceIntents";
import type { ComputerInUse } from "../../services/computers";
import { copyText } from "../clipboard";
import { PopupDialog } from "../Popup";
import { errorSentence, SettingsButton, SettingsInlineError } from "./settingsKit";

/*
 * The two pairing dialogs of ADR-0081. Pairing has one direction: the computer to be
 * used shows a code ("Add a device"), and the device that will use it enters the code
 * ("Use another computer"). Each dialog draws that direction and names the other
 * side's control as a path, so the user knows where the code goes.
 */

/** Names the relay at the moment pairing starts to use it: the public service (ADR 0062) or an Own Relay (ADR-0085). */
function ConnectionNote({ relayHost }: { relayHost?: string }) {
  return (
    <p className="settings-dialog-note">
      Encrypted end to end. Direct when the networks allow it, otherwise through{" "}
      {relayHost ? <>your relay {relayHost}</> : <>iroh’s public relays</>}, which cannot read it.
    </p>
  );
}

type PairingEnd = { name: string; icon: ReactNode } | { unknown: string };

const computerIcon = <Monitor aria-hidden="true" size={20} strokeWidth={1.6} />;

/** This device as one end of the connection. */
function thisDevice(device: "computer" | "phone"): PairingEnd {
  return device === "phone"
    ? { name: "This phone", icon: <Smartphone aria-hidden="true" size={20} strokeWidth={1.6} /> }
    : { name: "This computer", icon: computerIcon };
}

/** The connection as the object it is: which device uses which computer. An end not known yet is an empty slot. */
function PairingDirection({ from, to }: { from: PairingEnd; to: PairingEnd }) {
  const end = (value: PairingEnd) => "unknown" in value ? (
    <span className="settings-pairing-end" data-unknown="">
      <span className="settings-pairing-end-icon" />
      <small>{value.unknown}</small>
    </span>
  ) : (
    <span className="settings-pairing-end">
      <span className="settings-pairing-end-icon">{value.icon}</span>
      <small>{value.name}</small>
    </span>
  );
  const name = (value: PairingEnd) => "unknown" in value ? `a ${value.unknown.toLowerCase()}` : value.name;
  return (
    <div aria-label={`${name(from)} uses ${name(to)}`} className="settings-pairing-direction" role="img">
      {end(from)}
      <span aria-hidden="true" className="settings-pairing-uses"><ArrowRight size={16} /><small>uses</small></span>
      {end(to)}
    </div>
  );
}

/** The other side's step as the path of controls the user follows there. */
export function PairingPath({ lead, steps, tail }: { lead: string; steps: string[]; tail?: string }) {
  return (
    <p aria-label={`${lead} ${steps.join(", then ")}${tail ? `, ${tail}` : ""}`} className="settings-pairing-path">
      <span aria-hidden="true">{lead}</span>
      {steps.map((step, index) => (
        <span aria-hidden="true" className="settings-pairing-step" key={step}>
          {index ? <span className="settings-pairing-step-separator">›</span> : null}
          <span className="settings-pairing-crumb">{step}</span>
        </span>
      ))}
      {tail ? <span aria-hidden="true">{tail}</span> : null}
    </p>
  );
}

/** Shows a single-use code that lets one more device use `computer`, the computer in use. */
export function AddDeviceDialog({
  computer,
  devices,
  intents,
  onClose,
  relayHost,
}: {
  computer: string;
  devices: RemoteDeviceSummary[];
  intents: RemoteDeviceIntents;
  onClose: () => void;
  /** The Own Relay the code carries, if any. */
  relayHost?: string;
}) {
  const [invite, setInvite] = useState<DevicesCreateInviteResult>();
  const [error, setError] = useState<string>();
  const [copied, setCopied] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const knownDeviceIds = useRef(new Set(devices.map((device) => device.deviceId)));
  const expired = invite ? now >= invite.expiresAtMs : false;

  const create = () => {
    setError(undefined);
    setCopied(false);
    setInvite(undefined);
    intents.createInvite()
      .then((created) => { setNow(Date.now()); setInvite(created); })
      .catch((cause) => setError(errorSentence(cause, "Unable to create a pairing code.")));
  };
  // The code is valid only while this dialog shows it, so leaving withdraws it.
  useEffect(() => {
    create();
    return () => { void intents.cancelInvite().catch(() => undefined); };
  }, []);
  useEffect(() => {
    if (!invite || expired) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [expired, invite]);
  // A device that was not in the list when the dialog opened means the code was used.
  const paired = devices.find((device) => !knownDeviceIds.current.has(device.deviceId));
  const title = `Add a device to ${computer}`;

  return (
    <PopupDialog className="settings-confirm settings-pairing" label={title} onOpenChange={(open) => { if (!open) onClose(); }} open>
      <strong>{paired ? `${paired.name} can use ${computer}` : title}</strong>
      <PairingDirection
        from={paired ? { name: paired.name, icon: <MonitorSmartphone aria-hidden="true" size={20} strokeWidth={1.6} /> } : { unknown: "New device" }}
        to={{ name: computer, icon: computerIcon }}
      />
      {paired ? null : invite && !expired ? (
        <>
          <SecretQr label="Pairing code as a QR code" text={invite.code} />
          <code aria-label="Pairing code text" className="settings-code">{groupCode(invite.code)}</code>
          <p className="settings-dialog-note">Works once · expires in {formatCountdown(invite.expiresAtMs - now)}</p>
          <PairingPath
            lead="On the other device:"
            steps={["Settings", "Remote", "Use another computer"]}
            tail="or the first screen of a new phone."
          />
          <ConnectionNote relayHost={relayHost} />
        </>
      ) : expired ? (
        <p>This code expired. Make a new one to continue.</p>
      ) : error ? null : (
        <p role="status">Making a pairing code…</p>
      )}
      {error ? <SettingsInlineError>{error}</SettingsInlineError> : null}
      <footer>
        {paired ? null : invite && !expired ? (
          <SettingsButton
            icon={<Copy aria-hidden="true" size={13} />}
            onClick={() => {
              void copyText(invite.code).then(() => setCopied(true)).catch(() => setError("Unable to copy the code."));
            }}
          >
            {copied ? "Copied" : "Copy code"}
          </SettingsButton>
        ) : expired || error ? (
          <SettingsButton onClick={create}>New code</SettingsButton>
        ) : null}
        <SettingsButton onClick={onClose} variant={paired ? "primary" : "default"}>{paired ? "Done" : "Close"}</SettingsButton>
      </footer>
    </PopupDialog>
  );
}

/**
 * Spends the code another computer shows. The shell checks the code first, so a code
 * from the computer in use (named by `devices`) is refused before it is spent; success
 * moves the shell to that computer's work, and the work here keeps running.
 */
export function UseAnotherComputerDialog({
  device,
  devices,
  onClose,
  pair: spend,
}: {
  device: "computer" | "phone";
  /** The devices snapshot of the App Server in use; its key is absent until that computer first made a code. */
  devices: DeviceCollectionSnapshot | undefined;
  onClose: () => void;
  pair: (code: string, inUse?: ComputerInUse) => Promise<void>;
}) {
  const inUse = devices?.serverKey ? { key: devices.serverKey, name: devices.serverName } : undefined;
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const pair = async () => {
    if (busy || !code.trim()) return;
    setBusy(true);
    setError(undefined);
    try {
      await spend(code, inUse);
      onClose();
    } catch (cause) {
      setError(errorSentence(cause, "Pairing failed. Try again."));
      setBusy(false);
    }
  };
  return (
    <PopupDialog className="settings-confirm settings-pairing" label="Use another computer" onOpenChange={(open) => { if (!open && !busy) onClose(); }} open>
      <strong>Use another computer</strong>
      <PairingDirection from={thisDevice(device)} to={{ unknown: "Computer" }} />
      <textarea
        aria-label="Code from the computer"
        autoCapitalize="characters"
        autoComplete="off"
        autoFocus
        className="settings-code-input"
        onChange={(event) => setCode(event.currentTarget.value)}
        // A code is one word, so Enter spends it as soon as it is pasted.
        onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); void pair(); } }}
        placeholder="OAI1…"
        rows={3}
        spellCheck={false}
        value={code}
      />
      {error ? <SettingsInlineError>{error}</SettingsInlineError> : null}
      <PairingPath lead="Get the code on the computer:" steps={["Settings", "Remote", "Add a device"]} />
      <footer>
        <SettingsButton disabled={busy} onClick={onClose}>Cancel</SettingsButton>
        <SettingsButton busy={busy} disabled={!code.trim()} onClick={() => void pair()} variant="primary">{busy ? "Connecting…" : "Connect"}</SettingsButton>
      </footer>
    </PopupDialog>
  );
}

/**
 * A code another device reads with its camera: a Pairing Code or a relay key.
 * Dark-on-light regardless of theme: scanners need the contrast and the quiet border.
 */
export function SecretQr({ label, text }: { label: string; text: string }) {
  const { path, size } = useMemo(() => {
    const qr = encode(text, { border: 3, ecc: "M" });
    let d = "";
    qr.data.forEach((row, y) => row.forEach((dark, x) => { if (dark) d += `M${x} ${y}h1v1h-1z`; }));
    return { path: d, size: qr.size };
  }, [text]);
  return (
    <svg aria-label={label} className="settings-qr" role="img" shapeRendering="crispEdges" viewBox={`0 0 ${size} ${size}`}>
      <rect fill="#fff" height={size} width={size} />
      <path d={path} fill="#000" />
    </svg>
  );
}

/** Groups of four keep a long code readable when it is typed by hand. */
function groupCode(code: string) {
  return code.replace(/(.{4})(?=.)/g, "$1 ");
}

function formatCountdown(remainingMs: number) {
  const seconds = Math.max(0, Math.ceil(remainingMs / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}
