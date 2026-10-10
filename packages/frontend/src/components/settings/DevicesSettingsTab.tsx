import type {
  DeviceCollectionSnapshot,
  DevicesCreateInviteResult,
  DevicesPreviewJoinRequestResult,
  RemoteDeviceSummary,
} from "@openaide/app-server-client";
import { Copy, Keyboard, Laptop, QrCode, Smartphone, Trash2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { encode } from "uqr";

import type { RemoteDeviceIntents } from "../../intents/remoteDeviceIntents";
import { copyText } from "../clipboard";
import { PopupDialog } from "../Popup";
import { GeneralPreferenceRow, GeneralSection } from "./GeneralSettingsTab";
import { InlineNotice } from "./settingsPresentation";

type Dialog =
  | { kind: "invite" }
  | { kind: "join" }
  | { kind: "remove"; device: RemoteDeviceSummary };

/** Settings page that pairs and removes Remote Devices. The list itself is App Server state. */
export function DevicesSettingsTab({
  devices,
  intents,
}: {
  devices?: DeviceCollectionSnapshot;
  intents: RemoteDeviceIntents;
}) {
  const [dialog, setDialog] = useState<Dialog>();
  const close = () => setDialog(undefined);
  if (!devices) {
    return (
      <div className="general-settings-panel">
        <InlineNotice message="Loading devices…" />
      </div>
    );
  }
  const remoteDevices = devices.devices ?? [];
  return (
    <div className="general-settings-panel device-settings">
      <GeneralSection
        description="Use OpenAIDE on a phone or another computer. A paired device has full access to this OpenAIDE."
        id="settings-devices-add"
        label="Add a device"
      >
        <div className="general-preference-surface">
          <GeneralPreferenceRow
            action={<button className="device-action" onClick={() => setDialog({ kind: "invite" })} type="button">Show code</button>}
            detail="Scan or type it in OpenAIDE on the new device."
            icon={<QrCode size={16} />}
            label="Show a pairing code"
          />
          <GeneralPreferenceRow
            action={<button className="device-action" onClick={() => setDialog({ kind: "join" })} type="button">Enter code</button>}
            detail="Use this when the new device shows a code instead."
            icon={<Keyboard size={16} />}
            label="Enter a code from the device"
          />
        </div>
        {devices.remoteAccess === "off" ? (
          <p className="device-relay-notice">
            Adding a device turns on remote access. Connections are encrypted end to end. When the two
            devices cannot reach each other directly, traffic passes through public relays run by the
            iroh project, which see the network addresses and public keys of both ends but none of the content.
          </p>
        ) : null}
      </GeneralSection>

      <GeneralSection id="settings-devices-list" label="Devices">
        {devices.remoteAccess === "failed" ? (
          <p className="device-access-failed" role="alert">
            Remote access could not start, so paired devices cannot connect. Check the network and restart OpenAIDE.
          </p>
        ) : null}
        {devices.remoteAccess === "starting" ? <InlineNotice message="Starting remote access…" /> : null}
        <ul className="device-list" aria-label="Devices">
          <li className="device-row">
            <span className="general-preference-icon"><Laptop size={16} /></span>
            <span className="device-copy">
              <strong>{devices.serverName}</strong>
              <small>This computer · runs the App Server</small>
            </span>
          </li>
          {remoteDevices.map((device) => (
            <DeviceRow device={device} key={device.deviceId} onRemove={() => setDialog({ kind: "remove", device })} />
          ))}
        </ul>
        {remoteDevices.length === 0 ? <p className="device-empty">No other devices are paired.</p> : null}
      </GeneralSection>

      {dialog?.kind === "invite" ? <InviteDialog devices={remoteDevices} intents={intents} onClose={close} /> : null}
      {dialog?.kind === "join" ? <JoinDialog intents={intents} onClose={close} /> : null}
      {dialog?.kind === "remove" ? <RemoveDialog device={dialog.device} intents={intents} onClose={close} /> : null}
    </div>
  );
}

function DeviceRow({ device, onRemove }: { device: RemoteDeviceSummary; onRemove: () => void }) {
  const connection = device.connection ?? undefined;
  return (
    <li className="device-row">
      <span className="general-preference-icon"><Smartphone size={16} /></span>
      <span className="device-copy">
        <strong>
          {device.name}
          {device.model && device.model !== device.name ? <span className="device-model">{device.model}</span> : null}
        </strong>
        <small>
          {connection ? (
            <span className="device-connected">
              Connected · {connection.path === "direct" ? "direct" : "through a relay"}
              {connection.address ? ` · ${connection.address}` : ""}
            </span>
          ) : (
            <span>{device.lastSeenAtMs ? `Last seen ${formatMoment(device.lastSeenAtMs)}` : "Not connected yet"}</span>
          )}
        </small>
        <small>
          Added {formatMoment(device.addedAtMs)}
          {device.addedBy ? ` from ${device.addedBy}` : ""}
        </small>
      </span>
      <button aria-label={`Remove ${device.name}`} className="device-remove" onClick={onRemove} title="Remove device" type="button">
        <Trash2 size={14} />
      </button>
    </li>
  );
}

function InviteDialog({
  devices,
  intents,
  onClose,
}: {
  devices: RemoteDeviceSummary[];
  intents: RemoteDeviceIntents;
  onClose: () => void;
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
      .catch((cause) => setError(errorMessage(cause, "Unable to create a pairing code.")));
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

  return (
    <PopupDialog className="settings-reset-dialog device-dialog" label="Pairing code" onOpenChange={(open) => { if (!open) onClose(); }} open>
      <header>
        <QrCode size={17} />
        <div>
          <strong>{paired ? "Device added" : "Pair a new device"}</strong>
          <small>{paired ? `${paired.name} can now use this OpenAIDE.` : "In OpenAIDE on the new device, scan this code or type it in."}</small>
        </div>
      </header>
      {paired ? null : invite && !expired ? (
        <>
          <PairingQr code={invite.code} />
          <code className="device-code" aria-label="Pairing code text">{groupCode(invite.code)}</code>
          <p className="device-code-meta">
            Works once · expires in {formatCountdown(invite.expiresAtMs - now)}
          </p>
        </>
      ) : expired ? (
        <p>This code expired. Create a new one to continue.</p>
      ) : error ? (
        <p className="settings-reset-error" role="alert">{error}</p>
      ) : (
        <p>Creating a pairing code…</p>
      )}
      <footer>
        {paired ? null : invite && !expired ? (
          <button
            onClick={() => {
              void copyText(invite.code).then(() => setCopied(true)).catch(() => setError("Unable to copy the code."));
            }}
            type="button"
          >
            <Copy size={13} /> {copied ? "Copied" : "Copy code"}
          </button>
        ) : expired || error ? (
          <button onClick={create} type="button">New code</button>
        ) : null}
        <button onClick={onClose} type="button">{paired ? "Done" : "Close"}</button>
      </footer>
    </PopupDialog>
  );
}

function JoinDialog({ intents, onClose }: { intents: RemoteDeviceIntents; onClose: () => void }) {
  const [code, setCode] = useState("");
  const [preview, setPreview] = useState<DevicesPreviewJoinRequestResult>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const run = async (action: () => Promise<void>, fallback: string) => {
    setBusy(true);
    setError(undefined);
    try {
      await action();
    } catch (cause) {
      setError(errorMessage(cause, fallback));
    } finally {
      setBusy(false);
    }
  };
  const review = () => run(async () => setPreview(await intents.previewJoinRequest(code.trim())), "This code could not be read.");
  const approve = () => run(async () => {
    await intents.approveJoinRequest(code.trim());
    onClose();
  }, "Unable to add the device.");

  return (
    <PopupDialog className="settings-reset-dialog device-dialog" label="Add a device by code" onOpenChange={(open) => { if (!open && !busy) onClose(); }} open>
      <header>
        <Keyboard size={17} />
        <div>
          <strong>{preview ? `Add ${preview.name}?` : "Enter the code from the device"}</strong>
          <small>{preview ? "Check that this is your device." : "OpenAIDE on the new device shows it on its first screen."}</small>
        </div>
      </header>
      {preview ? (
        <>
          <dl className="device-preview">
            <div><dt>Device</dt><dd>{preview.name}{preview.model && preview.model !== preview.name ? ` · ${preview.model}` : ""}</dd></div>
            <div><dt>Connects to</dt><dd>{preview.serverName}</dd></div>
          </dl>
          <p>
            {preview.alreadyTrusted
              ? "This device is already paired. Adding it again tells it where to connect."
              : "It will have full access to this OpenAIDE: Tasks, files, and Agents. The name is reported by the device and is not verified."}
          </p>
        </>
      ) : (
        <textarea
          aria-label="Code from the device"
          autoCapitalize="characters"
          autoComplete="off"
          autoFocus
          className="device-code-input"
          onChange={(event) => setCode(event.currentTarget.value)}
          placeholder="OAJ1…"
          rows={3}
          spellCheck={false}
          value={code}
        />
      )}
      {error ? <p className="settings-reset-error" role="alert">{error}</p> : null}
      <footer>
        <button disabled={busy} onClick={preview ? () => { setPreview(undefined); setError(undefined); } : onClose} type="button">
          {preview ? "Back" : "Cancel"}
        </button>
        {preview ? (
          <button className="primary" disabled={busy} onClick={() => void approve()} type="button">
            {busy ? "Adding…" : "Add device"}
          </button>
        ) : (
          <button className="primary" disabled={busy || !code.trim()} onClick={() => void review()} type="button">Continue</button>
        )}
      </footer>
    </PopupDialog>
  );
}

function RemoveDialog({
  device,
  intents,
  onClose,
}: {
  device: RemoteDeviceSummary;
  intents: RemoteDeviceIntents;
  onClose: () => void;
}) {
  const [removing, setRemoving] = useState(false);
  const [error, setError] = useState<string>();
  const confirm = async () => {
    setRemoving(true);
    setError(undefined);
    try {
      await intents.remove(device.deviceId);
      onClose();
    } catch (cause) {
      setError(errorMessage(cause, "Unable to remove the device."));
      setRemoving(false);
    }
  };
  return (
    <PopupDialog className="settings-reset-dialog device-dialog device-dialog-danger" label="Remove device confirmation" onOpenChange={(open) => { if (!open && !removing) onClose(); }} open>
      <header>
        <Trash2 size={17} />
        <div>
          <strong>Remove {device.name}?</strong>
          <small>It is disconnected now and must be paired again to return.</small>
        </div>
      </header>
      {error ? <p className="settings-reset-error" role="alert">{error}</p> : null}
      <footer>
        <button disabled={removing} onClick={onClose} type="button">Cancel</button>
        <button className="danger" disabled={removing} onClick={() => void confirm()} type="button">
          {removing ? "Removing…" : "Remove device"}
        </button>
      </footer>
    </PopupDialog>
  );
}

/** Dark-on-light regardless of theme: scanners need the contrast and the quiet border. */
function PairingQr({ code }: { code: string }) {
  const { path, size } = useMemo(() => {
    const qr = encode(code, { border: 3, ecc: "M" });
    let d = "";
    qr.data.forEach((row, y) => row.forEach((dark, x) => { if (dark) d += `M${x} ${y}h1v1h-1z`; }));
    return { path: d, size: qr.size };
  }, [code]);
  return (
    <svg aria-label="Pairing code as a QR code" className="device-qr" role="img" shapeRendering="crispEdges" viewBox={`0 0 ${size} ${size}`}>
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

function formatMoment(timestampMs: number) {
  return new Date(timestampMs).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

function errorMessage(cause: unknown, fallback: string) {
  return cause instanceof Error && cause.message ? cause.message : fallback;
}
