import type { ClientReplacementImpactResult, ShellKind } from "@openaide/app-server-client";
import { useEffect, useState } from "react";
import type { SshComputerStatus, SshComputersCapability } from "../../services/computers";
import { PopupDialog } from "../Popup";
import { errorSentence, SettingsButton, SettingsConfirm, SettingsField, SettingsInlineError } from "./settingsKit";

/** The shell's report on the SSH Computer in use, read once per Settings visit. */
export function useSshStatus(ssh: SshComputersCapability | undefined): SshComputerStatus | undefined {
  const [status, setStatus] = useState<SshComputerStatus>();
  useEffect(() => {
    let live = true;
    void ssh?.status().then((next) => { if (live) setStatus(next); }).catch(() => undefined);
    return () => { live = false; };
  }, [ssh]);
  return status;
}

/** Adds an SSH Computer by host alias or `user@host`; success switches the shell to it. */
export function AddSshComputerDialog({ ssh, onClose }: { ssh: SshComputersCapability; onClose: () => void }) {
  const [destination, setDestination] = useState("");
  const [name, setName] = useState("");
  const [hosts, setHosts] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  useEffect(() => {
    let live = true;
    void ssh.hosts().then((next) => { if (live) setHosts(next); }).catch(() => undefined);
    return () => { live = false; };
  }, [ssh]);
  const add = async () => {
    setBusy(true);
    setError(undefined);
    try {
      await ssh.add(destination.trim(), name.trim() || undefined);
      onClose();
    } catch (cause) {
      setError(errorSentence(cause, "OpenAIDE could not add that computer. Try again."));
      setBusy(false);
    }
  };
  return (
    <PopupDialog className="settings-confirm" label="Add an SSH computer" onOpenChange={(open) => { if (!open && !busy) onClose(); }} open>
      <strong>Add an SSH computer</strong>
      <p>OpenAIDE installs itself on the Linux computer over SSH and keeps running there while agents work. It uses your SSH configuration and keys, and asks here for anything else. OpenAIDE then switches to that computer.</p>
      <SettingsField hint={hosts.length ? "Choose a host from your SSH configuration or type one." : undefined} label="Host">
        <input
          aria-label="Host"
          autoFocus
          className="settings-mono"
          list="settings-ssh-hosts"
          onChange={(event) => setDestination(event.currentTarget.value)}
          placeholder="devbox or user@host"
          spellCheck={false}
          value={destination}
        />
      </SettingsField>
      <datalist id="settings-ssh-hosts">
        {hosts.map((host) => <option key={host} value={host} />)}
      </datalist>
      <SettingsField label="Name (optional)">
        <input onChange={(event) => setName(event.currentTarget.value)} placeholder={destination.trim() || "Build server"} value={name} />
      </SettingsField>
      {error ? <SettingsInlineError>{error}</SettingsInlineError> : null}
      <footer>
        <SettingsButton disabled={busy} onClick={onClose}>Cancel</SettingsButton>
        <SettingsButton busy={busy} disabled={!destination.trim()} onClick={() => void add()} variant="primary">{busy ? "Adding…" : "Add and connect"}</SettingsButton>
      </footer>
    </PopupDialog>
  );
}

/**
 * Confirms replacing the older App Server on the SSH Computer in use. The
 * warning lists what the replacement interrupts, as that App Server reports it.
 */
export function UpdateSshComputerDialog({
  computer,
  impact,
  onClose,
  onUpdate,
  status,
}: {
  computer: string;
  impact?: () => Promise<ClientReplacementImpactResult>;
  onClose: () => void;
  /** Replaces the App Server there; the Desktop and phone shells each own how. */
  onUpdate: () => Promise<void>;
  status: SshComputerStatus;
}) {
  const [lines, setLines] = useState<string[]>();
  useEffect(() => {
    let live = true;
    const unknown = ["OpenAIDE could not tell what else is using it."];
    if (!impact) setLines(unknown);
    else void impact().then((result) => { if (live) setLines(describeImpact(result)); }).catch(() => { if (live) setLines(unknown); });
    return () => { live = false; };
  }, [impact]);
  return (
    <SettingsConfirm
      confirmLabel="Update and restart"
      detail={`${computer} runs OpenAIDE ${status.runningVersion}. Updating restarts it there and interrupts:`}
      label={`Update OpenAIDE on ${computer}?`}
      onClose={onClose}
      onConfirm={onUpdate}
      pendingLabel="Updating…"
      title={`Update OpenAIDE on ${computer}?`}
    >
      {lines ? <ul className="settings-impact">{lines.map((line, index) => <li key={index}>{line}</li>)}</ul> : <p>Checking what is running…</p>}
    </SettingsConfirm>
  );
}

const clientNames: Record<ShellKind, string> = {
  desktop: "OpenAIDE Desktop",
  web: "OpenAIDE in a browser",
  mobile: "OpenAIDE for Android",
  vscodeExtension: "OpenAIDE in VS Code",
  terminal: "The remote command in a terminal",
  keeper: "",
};

/** One line per interrupted client and kind of work; the keeper is OpenAIDE's own. */
export function describeImpact(impact: ClientReplacementImpactResult): string[] {
  const lines = (impact.otherClients ?? [])
    .filter((client) => client.kind !== "keeper")
    .map((client) => {
      const version = client.appVersion ? ` ${client.appVersion}` : "";
      return `${clientNames[client.kind]}${version}${client.reconnecting ? " (reconnecting)" : ""}`;
    });
  if (impact.activeTurns > 0) lines.push(`Agents working in ${count(impact.activeTurns, "Task")}`);
  if (impact.pendingTaskRequests > 0) lines.push(`${count(impact.pendingTaskRequests, "question")} waiting for an answer`);
  if (impact.openTerminals > 0) lines.push(`${count(impact.openTerminals, "terminal")} will close`);
  return lines.length ? lines : ["Nothing else is using it."];
}

function count(value: number, noun: string) {
  return `${value} ${noun}${value === 1 ? "" : "s"}`;
}
