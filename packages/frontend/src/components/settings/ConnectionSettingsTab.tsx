import { Battery, Bell, Bug, Laptop, QrCode, RefreshCcw, Smartphone } from "lucide-react";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { ConnectionCommand, ConnectionComputer, ConnectionSettings } from "../../services/connectionSettings";
import { GeneralPreferenceRow, GeneralSection, SettingsSwitch } from "./GeneralSettingsTab";
import { InlineNotice, SettingsSkeleton } from "./settingsPresentation";
import "./ConnectionSettingsTab.css";

export function ConnectionSettingsTab({ capability }: { capability: ConnectionSettings }) {
  const state = useSyncExternalStore(capability.subscribe, capability.snapshot, capability.snapshot);
  const [error, setError] = useState("");
  const [sending, setSending] = useState(false);
  const [forgetting, setForgetting] = useState<ConnectionComputer>();
  const mounted = useRef(true);
  const busy = sending || Boolean(state?.busy);
  const execute = async (command: ConnectionCommand) => {
    setError("");
    setSending(true);
    try { await capability.execute(command); }
    catch { if (mounted.current) setError("Phone settings did not respond. Try again, or reopen OpenAIDE."); }
    finally { if (mounted.current) setSending(false); }
  };

  useEffect(() => {
    mounted.current = true;
    void execute({ action: "state" });
    return () => { mounted.current = false; };
  }, [capability]);
  const button = (label: string, command: ConnectionCommand) => (
    <button type="button" className="settings-secondary-button" disabled={busy} onClick={() => { void execute(command); }}>{label}</button>
  );
  if (!state) return error ? (
    <div className="general-settings-panel"><p className="settings-notice" role="alert">{error}</p>{button("Try again", { action: "state" })}</div>
  ) : <SettingsSkeleton />;

  const checks = state.checks;
  const computers = state.computers ?? [];
  const needsTools = checks && ["node", "nodeVersion", "git", "npm", "agent", "runtime"].some(key => !checks[key]);
  return (
    <div className="general-settings-panel" aria-busy={busy}>
      {/* One list of places, with the one in use marked; adding a computer is a separate act. */}
      <GeneralSection id="settings-connection-workspace" label="Where agents work">
        <div className="general-preference-surface">
          <GeneralPreferenceRow label="This phone" icon={<Smartphone size={17} />} detail="Runs in Termux"
            action={state.remote ? button("Use", { action: "local" }) : <span className="connection-status">In use</span>} />
          {computers.map(computer => (
            <GeneralPreferenceRow key={computer.id} label={computer.name || "Remote computer"} icon={<Laptop size={17} />} detail="Remote computer"
              action={<span className="connection-actions">
                {computer.current ? <span className="connection-status">In use</span> : button("Use", { action: "use_computer", id: computer.id })}
                <button type="button" className="settings-secondary-button connection-forget" disabled={busy}
                  aria-label={`Forget ${computer.name || "this computer"}`}
                  onClick={() => setForgetting(computer)}>Forget</button>
              </span>} />
          ))}
        </div>
        {forgetting ? (
          <div className="connection-confirm" role="alertdialog" aria-label={`Forget ${forgetting.name || "this computer"}?`}>
            <p>
              Forget {forgetting.name || "this computer"}? This phone stops connecting to it.
              {forgetting.current ? " Work moves to this phone." : ""} Pair again to return.
            </p>
            <span className="connection-actions">
              <button type="button" className="settings-secondary-button" onClick={() => setForgetting(undefined)}>Keep</button>
              <button type="button" className="settings-secondary-button" disabled={busy}
                onClick={() => { const id = forgetting.id; setForgetting(undefined); void execute({ action: "forget_computer", id }); }}>Forget</button>
            </span>
          </div>
        ) : null}
      </GeneralSection>

      <GeneralSection id="settings-connection-add" label="Add">
        <div className="general-preference-surface">
          <GeneralPreferenceRow label={computers.length ? "Another computer" : "A computer"} icon={<QrCode size={17} />} detail="Pair with a code"
            action={button("Pair", { action: "pair_setup" })} />
        </div>
      </GeneralSection>

      {!state.remote ? (
        <GeneralSection id="settings-connection-background" label="Background work">
          <div className="general-preference-surface">
            <GeneralPreferenceRow label="Continue while locked" icon={<Battery size={17} />}
              action={<SettingsSwitch checked={state.background} disabled={busy} label="Continue while locked" onChange={enabled => { void execute({ action: "background", enabled }); }} />} />
            {state.background && (!state.appBattery || !state.termuxBattery) ? (
              <GeneralPreferenceRow label="Allow background activity" icon={<Battery size={17} />} detail="Unrestricted battery use for OpenAIDE and Termux"
                action={button("Open settings", { action: "battery" })} />
            ) : null}
            {state.background && !state.notifications ? (
              <GeneralPreferenceRow label="Work notification" icon={<Bell size={17} />} detail="See and stop background work"
                action={button("Allow", { action: "app_settings" })} />
            ) : null}
          </div>
          {state.background && state.batterySaver ? <InlineNotice message="Battery Saver is on. Android may pause work." /> : null}
        </GeneralSection>
      ) : null}

      <details className="general-settings-section">
        <summary className="general-settings-section-heading">Advanced</summary>
        <div className="general-preference-surface">
          {!state.remote ? <>
            <GeneralPreferenceRow label="Check phone setup" icon={<Smartphone size={17} />} action={button("Check", { action: "check" })} />
            {!state.termux ? <GeneralPreferenceRow label="Install Termux" icon={<Smartphone size={17} />} action={button("Get Termux", { action: "get_termux" })} />
              : !state.permission ? <GeneralPreferenceRow label="Termux access" icon={<Smartphone size={17} />} action={button("Allow", { action: "grant" })} /> : null}
            {needsTools ? <GeneralPreferenceRow label="Required tools" detail="Your projects are kept" icon={<RefreshCcw size={17} />} action={button("Install", { action: "install" })} /> : null}
            {checks && !needsTools && !checks.claude ? <GeneralPreferenceRow label="Claude" detail="Not installed on this phone" icon={<Smartphone size={17} />} action={button("Install", { action: "install" })} /> : null}
            {checks && checks.agent && !checks.agentVersion ? <GeneralPreferenceRow label="Agent version" detail="Requires Codex 0.153.3" icon={<Smartphone size={17} />} action={button("Open Termux", { action: "termux" })} /> : null}
            {checks && !checks.authenticated ? <GeneralPreferenceRow label="Agent sign-in" detail="Paste the copied command in Termux" icon={<Smartphone size={17} />} action={button("Sign in", { action: "signin" })} /> : null}
            {checks && !needsTools && checks.authenticated && checks.agentVersion ? <InlineNotice message="Your phone is ready for local work." /> : null}
          </> : null}
          <GeneralPreferenceRow label="Connection diagnostics" detail="No conversations or credentials" icon={<Bug size={17} />} action={button("Share", { action: "diagnostics" })} />
        </div>
      </details>
      {state.notice ? <div role="status"><InlineNotice message={state.notice} /></div> : null}
      {error ? <p className="settings-notice" role="alert">{error}</p> : null}
    </div>
  );
}
