import { Battery, Bell, Bug, Laptop, QrCode, RefreshCcw, Smartphone } from "lucide-react";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { ConnectionCommand, ConnectionSettings } from "../../services/connectionSettings";
import { GeneralPreferenceRow, GeneralSection, SettingsSwitch } from "./GeneralSettingsTab";
import { InlineNotice, SettingsSkeleton } from "./settingsPresentation";
import "./ConnectionSettingsTab.css";

export function ConnectionSettingsTab({ capability }: { capability: ConnectionSettings }) {
  const state = useSyncExternalStore(capability.subscribe, capability.snapshot, capability.snapshot);
  const [error, setError] = useState("");
  const [sending, setSending] = useState(false);
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
  const needsTools = checks && ["node", "nodeVersion", "git", "npm", "agent", "runtime"].some(key => !checks[key]);
  return (
    <div className="general-settings-panel" aria-busy={busy}>
      <GeneralSection id="settings-connection-workspace" label="Connected to">
        <div className="general-preference-surface">
          <GeneralPreferenceRow label={state.remote ? state.computer || "Remote computer" : "This phone"}
            icon={state.remote ? <Laptop size={17} /> : <Smartphone size={17} />}
            detail={state.remote ? "Remote computer" : "Runs in Termux"}
            action={<span className="connection-status">Connected</span>} />
        </div>
      </GeneralSection>

      <GeneralSection id="settings-connection-switch" label="Switch">
        <div className="general-preference-surface">
          {state.remote ? (
            <>
              <GeneralPreferenceRow label="This phone" icon={<Smartphone size={17} />}
                action={button("Connect to this phone", { action: "local" })} />
              <GeneralPreferenceRow label={state.computer || "This computer"} icon={<Laptop size={17} />} detail="Stop connecting from this phone"
                action={button("Forget", { action: "forget" })} />
            </>
          ) : state.paired ? (
            <GeneralPreferenceRow label={state.computer || "Paired computer"} icon={<Laptop size={17} />} detail="Paired"
              action={button("Connect to computer", { action: "paired" })} />
          ) : null}
          <GeneralPreferenceRow label={state.paired || state.remote ? "Another computer" : "A computer"} icon={<QrCode size={17} />}
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
