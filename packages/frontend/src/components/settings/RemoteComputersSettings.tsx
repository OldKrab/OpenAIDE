import { Laptop, Link2, Monitor } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { RemoteComputer, RemoteComputersCapability } from "../../services/frontendShell";
import { GeneralPreferenceRow } from "./GeneralSettingsTab";
import "./ConnectionSettingsTab.css";

/**
 * Chooses whose App Server this Desktop uses: its own, or a paired computer's.
 * The shell owns the choice and restarts to apply it, so this view only lists
 * what the shell reports and forwards what the user picked.
 */
// TODO: the phone's Connection tab renders the same list from its own
// capability. Merge both into one place when Settings is restructured, instead
// of growing either copy.
export function RemoteComputersSettings({ capability }: { capability: RemoteComputersCapability }) {
  const [computers, setComputers] = useState<RemoteComputer[]>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [code, setCode] = useState("");
  const [pairing, setPairing] = useState(false);
  const [forgetting, setForgetting] = useState<RemoteComputer>();
  const mounted = useRef(true);
  const refresh = async () => {
    try {
      const next = await capability.list();
      if (mounted.current) setComputers(next);
    } catch {
      if (mounted.current) setError("OpenAIDE could not read its paired computers. Try again.");
    }
  };
  // Every action but a refresh ends in a restart when it succeeds, so a
  // returned promise means nothing changed or the shell refused.
  const run = async (action: () => Promise<void>, failure: string) => {
    setError("");
    setBusy(true);
    try {
      await action();
      await refresh();
    } catch (reason) {
      // The shell answers with a sentence written for the user.
      if (mounted.current) setError(typeof reason === "string" && reason ? reason : failure);
    } finally {
      if (mounted.current) setBusy(false);
    }
  };
  useEffect(() => {
    mounted.current = true;
    void refresh();
    return () => { mounted.current = false; };
  }, [capability]);

  const remote = computers?.some((computer) => computer.current) ?? false;
  const use = (id?: string) => run(() => capability.use(id), "OpenAIDE could not switch. Try again.");
  return (
    <div aria-busy={busy}>
      <div className="general-preference-surface">
        <GeneralPreferenceRow label="This computer" icon={<Monitor size={17} />} detail="Projects, tools, and Agents here"
          action={remote
            ? <button type="button" className="settings-secondary-button" disabled={busy} onClick={() => { void use(); }}>Use</button>
            : <span className="connection-status">In use</span>} />
        {(computers ?? []).map((computer) => (
          <GeneralPreferenceRow key={computer.id} label={computer.name} icon={<Laptop size={17} />} detail="Paired computer"
            action={<span className="connection-actions">
              {computer.current
                ? <span className="connection-status">In use</span>
                : <button type="button" className="settings-secondary-button" disabled={busy} onClick={() => { void use(computer.id); }}>Use</button>}
              <button type="button" className="settings-secondary-button" disabled={busy}
                aria-label={`Forget ${computer.name}`} onClick={() => setForgetting(computer)}>Forget</button>
            </span>} />
        ))}
        <GeneralPreferenceRow label="Another computer" icon={<Link2 size={17} />} detail="Pair with a code from its Settings → Devices"
          action={<button type="button" className="settings-secondary-button" disabled={busy} onClick={() => setPairing(true)}>Pair</button>} />
      </div>
      {forgetting ? (
        <div className="connection-confirm" role="alertdialog" aria-label={`Forget ${forgetting.name}?`}>
          <p>
            Forget {forgetting.name}? This computer stops connecting to it.
            {forgetting.current ? " OpenAIDE restarts and works on this computer." : ""}
            {" "}It stays in that computer’s device list until you remove it there.
          </p>
          <span className="connection-actions">
            <button type="button" className="settings-secondary-button" onClick={() => setForgetting(undefined)}>Keep</button>
            <button type="button" className="settings-secondary-button" disabled={busy}
              onClick={() => { const id = forgetting.id; setForgetting(undefined); void run(() => capability.forget(id), "OpenAIDE could not forget that computer. Try again."); }}>Forget</button>
          </span>
        </div>
      ) : null}
      {pairing ? (
        <form className="connection-confirm" aria-label="Pair with another computer"
          onSubmit={(event) => { event.preventDefault(); void run(() => capability.pair(code), "Pairing failed. Try again."); }}>
          <p>
            On the other computer, open Settings → Devices, choose Show code, then Copy code, and paste it here.
            OpenAIDE restarts here and opens that computer’s projects.
          </p>
          <textarea aria-label="Pairing code" autoFocus className="device-code-input" onChange={(event) => setCode(event.currentTarget.value)}
            placeholder="OAI1…" rows={2} spellCheck={false} value={code} />
          <p className="connection-note">
            The two computers connect directly when the network allows and through public relays run by the iroh project otherwise. Everything is encrypted end to end; no account is involved.
          </p>
          <span className="connection-actions">
            <button type="button" className="settings-secondary-button" disabled={busy} onClick={() => { setPairing(false); setCode(""); setError(""); }}>Cancel</button>
            <button type="submit" className="settings-secondary-button" disabled={busy || !code.trim()}>{busy ? "Pairing…" : "Pair"}</button>
          </span>
        </form>
      ) : null}
      {error ? <p className="settings-notice" role="alert">{error}</p> : null}
    </div>
  );
}
