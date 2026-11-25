import { AppServerProtocolError, type DeviceCollectionSnapshot, type DevicesRelayAddPersonResult, type RelayPerson } from "@openaide/app-server-client";
import { Copy, Globe, Plus, RadioTower, UserRound } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import type { RemoteDeviceIntents } from "../../intents/remoteDeviceIntents";
import { copyText } from "../clipboard";
import { PopupDialog } from "../Popup";
import { PairingPath, SecretQr } from "./devicePairingDialogs";
import { settingsInfo } from "./settingsInfo";
import { errorSentence, SettingsButton, SettingsConfirm, SettingsField, SettingsInlineError, SettingsNotice } from "./settingsKit";
import type { SettingsGroup, SettingsRow } from "./settingsModel";

/*
 * The relay side of Settings › Remote (ADR-0085): which relay devices that cannot reach
 * this computer directly go through, and, when this computer's relay key may manage the
 * relay, the people it admits. App Server owns the relay and its people; this only shows
 * them and sends intent. A relay key is a secret: it is typed into the key field or shown
 * once after adding a person, and never kept, logged, or rendered anywhere else.
 */

export type RelaySettingsProps = {
  /** The Remote section is on screen, so the relay's people are worth reading. */
  active: boolean;
  intents: RemoteDeviceIntents | undefined;
  devices: DeviceCollectionSnapshot | undefined;
  /** The computer whose relay this is, as the Devices group names it. */
  owner: string;
};

export type RelaySettings = { groups: SettingsGroup[]; overlay: ReactNode };

type RelayDialog =
  | { kind: "set-relay"; replacing: boolean }
  | { kind: "use-public" }
  | { kind: "add-person" }
  | { kind: "revoke"; person: RelayPerson };

/** The control a person pastes a relay key into, named the same in the add-person result. */
const useOwnRelayLabel = "Use own relay";

export function useRelaySettings({ active, intents, devices, owner }: RelaySettingsProps): RelaySettings {
  const [dialog, setDialog] = useState<RelayDialog>();
  const [changed, setChanged] = useState<string>();
  const relay = devices?.ownRelay;
  const refused = relay?.access === "refused";
  const manages = Boolean(relay?.canManage && !refused);
  const people = useRelayPeople(intents, active && manages, relay?.host);
  if (!intents || !devices) return { groups: [], overlay: undefined };

  const close = () => setDialog(undefined);
  const open = (next: RelayDialog) => {
    setChanged(undefined);
    setDialog(next);
  };
  const paired = devices.devices?.length ?? 0;
  const setRelay = async (relayKey: string | undefined, host: string) => {
    const { removedDevices } = await intents.setRelay(relayKey);
    setChanged(`Devices now go through ${host}.${removedDevices ? ` ${countDevices(removedDevices)} removed; pair ${removedDevices === 1 ? "it" : "them"} again.` : ""}`);
  };

  const groups = [relayGroup(devices, owner, open, changed)];
  if (manages && relay) groups.push(peopleGroup(people, open));
  return {
    groups,
    overlay: dialog ? (
      <>
        {dialog.kind === "set-relay" ? (
          <RelayKeyDialog
            deviceCount={paired}
            onClose={close}
            onSet={(relayKey) => setRelay(relayKey, "your relay")}
            replacing={dialog.replacing}
          />
        ) : null}
        {dialog.kind === "use-public" ? (
          <SettingsConfirm
            confirmLabel="Use free relays"
            danger={paired > 0}
            detail={paired ? removalWarning(paired) : "Devices that can't connect directly will go through the free public relays."}
            label="Use the free relays"
            onClose={close}
            onConfirm={() => setRelay(undefined, "the free public relays")}
            pendingLabel="Switching…"
            title="Go back to the free relays?"
          />
        ) : null}
        {dialog.kind === "add-person" ? <AddPersonDialog intents={intents} onAdded={people.reload} onClose={close} /> : null}
        {dialog.kind === "revoke" ? (
          <SettingsConfirm
            confirmLabel="Revoke"
            detail="Their computers stop using this relay at once."
            label="Revoke relay key"
            onClose={close}
            onConfirm={async () => {
              await intents.relayRevokePerson(dialog.person.name);
              people.reload();
            }}
            pendingLabel="Revoking…"
            title={`Revoke ${dialog.person.name}?`}
          />
        ) : null}
      </>
    ) : undefined,
  };
}

function relayGroup(devices: DeviceCollectionSnapshot, owner: string, open: (dialog: RelayDialog) => void, changed: string | undefined): SettingsGroup {
  const relay = devices.ownRelay;
  const refused = relay?.access === "refused";
  const row: SettingsRow = relay ? {
    id: "relay-own",
    label: relay.host,
    description: "Your relay",
    keywords: "relay own relay key host change free public",
    leading: <RadioTower aria-hidden="true" size={18} strokeWidth={1.6} />,
    status: refused ? { tone: "warn", text: "Refused" } : undefined,
    error: refused ? "The relay no longer accepts this computer." : undefined,
    control: (
      <>
        <SettingsButton onClick={() => open({ kind: "use-public" })} variant="quiet">Use free relays</SettingsButton>
        <SettingsButton onClick={() => open({ kind: "set-relay", replacing: true })} variant={refused ? "primary" : "default"}>
          {refused ? "Enter new key" : "Change key"}
        </SettingsButton>
      </>
    ),
  } : {
    id: "relay-public",
    label: "Free public relays",
    description: `For devices that can't reach ${owner} directly`,
    keywords: "relay iroh public free own relay key",
    leading: <Globe aria-hidden="true" size={18} strokeWidth={1.6} />,
    control: <SettingsButton onClick={() => open({ kind: "set-relay", replacing: false })}>{useOwnRelayLabel}</SettingsButton>,
  };
  return {
    id: "relay",
    title: "Relay",
    info: settingsInfo.relay,
    notice: changed ? <SettingsNotice>{changed}</SettingsNotice> : undefined,
    rows: [row],
  };
}

type RelayPeople = { people: RelayPerson[] | undefined; error: string | undefined; reload(): void };

/**
 * The relay's people are not in the Devices snapshot, so they are read each time the
 * section shows, when the relay changes, and after this client adds or revokes someone.
 */
function useRelayPeople(intents: RemoteDeviceIntents | undefined, enabled: boolean, host: string | undefined): RelayPeople {
  const [loaded, setLoaded] = useState<{ host: string; people: RelayPerson[] }>();
  const [error, setError] = useState<string>();
  const [reads, setReads] = useState(0);
  useEffect(() => {
    if (!enabled || !intents || !host) return;
    let current = true;
    setError(undefined);
    intents.relayPeople().then(
      (people) => { if (current) setLoaded({ host, people }); },
      (cause) => { if (current) setError(errorSentence(cause, "The relay's people could not be read.")); },
    );
    return () => { current = false; };
  }, [enabled, intents, host, reads]);
  return {
    // A list read from another relay is not this one's.
    people: loaded?.host === host ? loaded?.people : undefined,
    error,
    reload: () => setReads((count) => count + 1),
  };
}

function peopleGroup(people: RelayPeople, open: (dialog: RelayDialog) => void): SettingsGroup {
  return {
    id: "relay-people",
    title: "My relay",
    info: settingsInfo.myRelay,
    actions: (
      <SettingsButton icon={<Plus aria-hidden="true" size={14} />} onClick={() => open({ kind: "add-person" })}>Add person</SettingsButton>
    ),
    notice: people.error ? (
      <SettingsInlineError>
        {people.error}{" "}
        <SettingsButton onClick={people.reload} variant="quiet">Try again</SettingsButton>
      </SettingsInlineError>
    ) : undefined,
    empty: people.error ? undefined : people.people ? "No one yet." : "Loading…",
    rows: (people.error ? [] : people.people ?? []).map((person): SettingsRow => ({
      id: `relay-person-${person.name}`,
      label: person.name,
      description: [person.thisComputer ? "This computer" : "", person.canManage ? "Can add and remove people" : ""].filter(Boolean).join(" · ") || undefined,
      keywords: "relay person people key revoke manage",
      leading: <UserRound aria-hidden="true" size={18} strokeWidth={1.6} />,
      // This computer's own key is what keeps it reachable; the App Server refuses revoking it.
      control: person.thisComputer ? undefined : (
        <SettingsButton ariaLabel={`Revoke ${person.name}`} danger onClick={() => open({ kind: "revoke", person })} variant="quiet">Revoke</SettingsButton>
      ),
    })),
  };
}

/** Takes a relay key line for this computer. The App Server asks the relay first, which can take a few seconds. */
function RelayKeyDialog({
  deviceCount,
  onClose,
  onSet,
  replacing,
}: {
  deviceCount: number;
  onClose: () => void;
  onSet: (relayKey: string) => Promise<void>;
  replacing: boolean;
}) {
  const [relayKey, setRelayKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<{ message: string; field?: string }>();
  const title = replacing ? "Change relay key" : "Use your own relay";
  const submit = async () => {
    if (busy || !relayKey.trim()) return;
    setBusy(true);
    setError(undefined);
    try {
      await onSet(relayKey.trim());
      onClose();
    } catch (cause) {
      setError({ message: errorSentence(cause, "The relay could not be set. Try again."), field: errorField(cause) });
      setBusy(false);
    }
  };
  return (
    <PopupDialog className="settings-confirm settings-form-dialog" label={title} onOpenChange={(open) => { if (!open && !busy) onClose(); }} open>
      <strong>{title}</strong>
      <SettingsField hint="From the person who runs the relay" label="Relay key">
        {/* A secret: no autofill, spelling, or capitalization help, and nothing that would keep or send it elsewhere. */}
        <textarea
          aria-invalid={error?.field === "relay" || undefined}
          aria-label="Relay key"
          autoCapitalize="off"
          autoComplete="off"
          autoCorrect="off"
          autoFocus
          className="settings-code-input"
          data-1p-ignore=""
          data-lpignore="true"
          onChange={(event) => setRelayKey(event.currentTarget.value)}
          // A key is one line, so Enter submits it as soon as it is pasted.
          onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); void submit(); } }}
          placeholder="https://…"
          rows={3}
          spellCheck={false}
          value={relayKey}
        />
      </SettingsField>
      <p>{deviceCount ? removalWarning(deviceCount) : "Changing the relay later means pairing every device again."}</p>
      {error ? <SettingsInlineError>{error.message}</SettingsInlineError> : null}
      <footer>
        <SettingsButton disabled={busy} onClick={onClose}>Cancel</SettingsButton>
        <SettingsButton busy={busy} danger={deviceCount > 0} disabled={!relayKey.trim()} onClick={() => void submit()} variant={deviceCount ? "default" : "primary"}>
          {busy ? "Checking the relay…" : replacing ? "Change key" : "Use relay"}
        </SettingsButton>
      </footer>
    </PopupDialog>
  );
}

/** Makes a relay key for one more person, then shows it once to be handed over. */
function AddPersonDialog({ intents, onAdded, onClose }: { intents: RemoteDeviceIntents; onAdded: () => void; onClose: () => void }) {
  const [name, setName] = useState("");
  const [canManage, setCanManage] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<{ message: string; field?: string }>();
  const [added, setAdded] = useState<DevicesRelayAddPersonResult>();
  const [copied, setCopied] = useState(false);
  const submit = async () => {
    if (busy || !name.trim()) return;
    setBusy(true);
    setError(undefined);
    try {
      setAdded(await intents.relayAddPerson(name.trim(), canManage));
      onAdded();
    } catch (cause) {
      setError({ message: errorSentence(cause, "The person could not be added. Try again."), field: errorField(cause) });
    } finally {
      setBusy(false);
    }
  };

  if (added) {
    const person = added.person.name;
    return (
      <PopupDialog className="settings-confirm settings-pairing" label={`Relay key for ${person}`} onOpenChange={(open) => { if (!open) onClose(); }} open>
        <strong>Relay key for {person}</strong>
        <SecretQr label="Relay key as a QR code" text={added.key} />
        <code aria-label="Relay key text" className="settings-code">{added.key}</code>
        <PairingPath lead={`Give it to ${person}, who enters it in`} steps={["Settings", "Remote", useOwnRelayLabel]} />
        {error ? <SettingsInlineError>{error.message}</SettingsInlineError> : null}
        <footer>
          <SettingsButton
            icon={<Copy aria-hidden="true" size={13} />}
            onClick={() => {
              void copyText(added.key).then(() => setCopied(true)).catch(() => setError({ message: "Unable to copy the key." }));
            }}
          >
            {copied ? "Copied" : "Copy key"}
          </SettingsButton>
          <SettingsButton onClick={onClose} variant="primary">Done</SettingsButton>
        </footer>
      </PopupDialog>
    );
  }
  return (
    <PopupDialog className="settings-confirm settings-form-dialog" label="Add a person" onOpenChange={(open) => { if (!open && !busy) onClose(); }} open>
      <strong>Add a person</strong>
      <SettingsField label="Name">
        <input
          aria-invalid={error?.field === "name" || undefined}
          aria-label="Name"
          autoComplete="off"
          autoFocus
          onChange={(event) => setName(event.currentTarget.value)}
          onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); void submit(); } }}
          value={name}
        />
      </SettingsField>
      <label className="settings-check">
        <input checked={canManage} onChange={(event) => setCanManage(event.currentTarget.checked)} type="checkbox" />
        Can add and remove people
      </label>
      {error ? <SettingsInlineError>{error.message}</SettingsInlineError> : null}
      <footer>
        <SettingsButton disabled={busy} onClick={onClose}>Cancel</SettingsButton>
        <SettingsButton busy={busy} disabled={!name.trim()} onClick={() => void submit()} variant="primary">{busy ? "Adding…" : "Add person"}</SettingsButton>
      </footer>
    </PopupDialog>
  );
}

function removalWarning(count: number) {
  return count === 1
    ? "This removes the paired device. It must be paired again."
    : `This removes all ${count} paired devices. Each must be paired again.`;
}

function countDevices(count: number) {
  return count === 1 ? "1 device was" : `${count} devices were`;
}

/** The request field an App Server refusal names, so the dialog can mark that input. */
function errorField(cause: unknown) {
  return cause instanceof AppServerProtocolError ? cause.protocolError.target?.field ?? undefined : undefined;
}
