import type {
  DevicesCreateInviteResult,
  DevicesPreviewJoinRequestResult,
  RemoteDeviceSummary,
} from "@openaide/app-server-client";
import { Copy } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { encode } from "uqr";
import type { RemoteDeviceIntents } from "../../intents/remoteDeviceIntents";
import { copyText } from "../clipboard";
import { PopupDialog } from "../Popup";
import { errorSentence, SettingsButton, SettingsInlineError } from "./settingsKit";

/** Names the public service at the moment pairing starts to use it (ADR 0062). */
function ConnectionNote() {
  return (
    <p className="settings-dialog-note">
      Encrypted end to end. Direct when the networks allow it, otherwise through iroh’s public relays, which cannot
      read it.
    </p>
  );
}

/** Shows a single-use code for a new device to scan or type. */
export function InviteDialog({
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

  return (
    <PopupDialog className="settings-confirm settings-pairing" label="Pairing code" onOpenChange={(open) => { if (!open) onClose(); }} open>
      <strong>{paired ? "Device added" : "Pair a new device"}</strong>
      <p>{paired ? `${paired.name} can now use this OpenAIDE.` : "In OpenAIDE on the new device, scan this code or type it in."}</p>
      {paired ? null : invite && !expired ? (
        <>
          <PairingQr code={invite.code} />
          <code aria-label="Pairing code text" className="settings-code">{groupCode(invite.code)}</code>
          <p className="settings-dialog-note">Works once · expires in {formatCountdown(invite.expiresAtMs - now)}</p>
          <ConnectionNote />
        </>
      ) : expired ? (
        <p>This code expired. Create a new one to continue.</p>
      ) : error ? null : (
        <p role="status">Creating a pairing code…</p>
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

/** Trusts a device from the code it shows, after the user has seen what the code names. */
export function JoinDialog({ intents, onClose }: { intents: RemoteDeviceIntents; onClose: () => void }) {
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
      setError(errorSentence(cause, fallback));
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
    <PopupDialog className="settings-confirm settings-pairing" label="Add a device by code" onOpenChange={(open) => { if (!open && !busy) onClose(); }} open>
      <strong>{preview ? `Add ${preview.name}?` : "Enter the code from the device"}</strong>
      <p>{preview ? "Check that this is your device." : "OpenAIDE on the new device shows it on its first screen."}</p>
      {preview ? (
        <>
          <dl className="settings-facts">
            <div><dt>Device</dt><dd>{preview.name}{preview.model && preview.model !== preview.name ? ` · ${preview.model}` : ""}</dd></div>
            <div><dt>Connects to</dt><dd>{preview.serverName}</dd></div>
          </dl>
          <p>
            {preview.alreadyTrusted
              ? "This device is already paired. Adding it again tells it where to connect."
              : "It will have full access to this OpenAIDE: Tasks, files, and Agents. The name is reported by the device and is not verified."}
          </p>
          <ConnectionNote />
        </>
      ) : (
        <textarea
          aria-label="Code from the device"
          autoCapitalize="characters"
          autoComplete="off"
          autoFocus
          className="settings-code-input"
          onChange={(event) => setCode(event.currentTarget.value)}
          placeholder="OAJ1…"
          rows={3}
          spellCheck={false}
          value={code}
        />
      )}
      {error ? <SettingsInlineError>{error}</SettingsInlineError> : null}
      <footer>
        <SettingsButton disabled={busy} onClick={preview ? () => { setPreview(undefined); setError(undefined); } : onClose}>
          {preview ? "Back" : "Cancel"}
        </SettingsButton>
        {preview ? (
          <SettingsButton busy={busy} onClick={() => void approve()} variant="primary">{busy ? "Adding…" : "Add device"}</SettingsButton>
        ) : (
          <SettingsButton disabled={busy || !code.trim()} onClick={() => void review()} variant="primary">Continue</SettingsButton>
        )}
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
    <svg aria-label="Pairing code as a QR code" className="settings-qr" role="img" shapeRendering="crispEdges" viewBox={`0 0 ${size} ${size}`}>
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
