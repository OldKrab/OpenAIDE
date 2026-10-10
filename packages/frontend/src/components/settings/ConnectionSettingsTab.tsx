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
  const needsTools = checks && ["node", "nodeVersion", "git", "npm", "codex", "runtime", "frontend", "supervisor"].some(key => !checks[key]);
  return (
    <div className="general-settings-panel" aria-busy={busy}>
      <GeneralSection id="settings-connection-workspace" label="Connected to" description="Your projects and conversations stay on the device where agents run.">
        <div className="general-preference-surface connection-current">
          <GeneralPreferenceRow label={state.remote ? state.computer || "Remote computer" : "This phone"}
            icon={state.remote ? <Laptop size={17} /> : <Smartphone size={17} />}
            detail={state.remote ? "Work runs on your computer, even when your phone is locked." : "Agents run in Termux on this phone. OpenAIDE starts Termux when needed."}
            action={<span className="connection-status">Connected</span>} />
        </div>
      </GeneralSection>

      <GeneralSection id="settings-connection-switch" label="Switch"
        description={state.paired ? "Forgetting a computer only stops this phone from connecting. Remove the phone in Settings → Devices on the computer to end its access." : "No account or password. On your computer, open Settings → Devices and choose Show code, then scan it with this phone."}>
        <div className="general-preference-surface">
          {state.remote ? (
            <>
              <GeneralPreferenceRow label="Use this phone" icon={<Smartphone size={17} />} detail="Switch to the projects and conversations stored on this phone. Remote work is not stopped."
                action={button("Connect to this phone", { action: "local" })} />
              <GeneralPreferenceRow label={`Forget ${state.computer || "this computer"}`} icon={<Laptop size={17} />} detail="Stop connecting to this computer from this phone."
                action={button("Forget", { action: "forget" })} />
            </>
          ) : state.paired ? (
            <GeneralPreferenceRow label={state.computer || "Paired computer"} icon={<Laptop size={17} />} detail="This phone is still trusted by this computer."
              action={button("Connect to computer", { action: "paired" })} />
          ) : null}
          <GeneralPreferenceRow label={state.paired || state.remote ? "Pair another computer" : "Pair with a computer"} icon={<QrCode size={17} />}
            detail="Scan or paste a code from OpenAIDE on a computer."
            action={button(state.paired || state.remote ? "Pair another" : "Pair", { action: "pair_setup" })} />
        </div>
      </GeneralSection>

      {!state.remote ? (
        <GeneralSection id="settings-connection-background" label="Background work">
          <div className="general-preference-surface">
            <GeneralPreferenceRow label="Continue while locked" icon={<Battery size={17} />} detail="Keep active work running when you leave the app or lock your phone. Extra protection stops when work finishes."
              action={<SettingsSwitch checked={state.background} disabled={busy} label="Continue while locked" onChange={enabled => { void execute({ action: "background", enabled }); }} />} />
            {state.background && (!state.appBattery || !state.termuxBattery) ? (
              <GeneralPreferenceRow label="Allow background activity" icon={<Battery size={17} />} detail="Set OpenAIDE and Termux to unrestricted battery use so Android does not pause active work."
                action={button("Open settings", { action: "battery" })} />
            ) : null}
            {state.background && !state.notifications ? (
              <GeneralPreferenceRow label="Work notification" icon={<Bell size={17} />} detail="Allow notifications to see and stop background work."
                action={button("Allow", { action: "app_settings" })} />
            ) : null}
          </div>
          {state.background && state.batterySaver ? <InlineNotice message="Battery Saver is on. Android may delay work even with background activity allowed." /> : null}
        </GeneralSection>
      ) : <InlineNotice message="Keep your computer awake and connected. Remote work continues there when you close OpenAIDE or lock your phone." />}

      <details className="general-settings-section">
        <summary className="general-settings-section-heading">Advanced</summary>
        <div className="general-preference-surface">
          {!state.remote ? <>
            <GeneralPreferenceRow label="Check phone setup" detail="Check Termux access, required tools, and agent sign-in." icon={<Smartphone size={17} />} action={button("Check", { action: "check" })} />
            {!state.termux ? <GeneralPreferenceRow label="Install Termux" detail="Termux runs your local workspace." icon={<Smartphone size={17} />} action={button("Get Termux", { action: "get_termux" })} />
              : !state.permission ? <GeneralPreferenceRow label="Termux access" detail="Allow OpenAIDE to start local work for you." icon={<Smartphone size={17} />} action={button("Allow", { action: "grant" })} /> : null}
            {needsTools ? <GeneralPreferenceRow label="Required tools" detail="Install missing workspace tools without deleting your projects." icon={<RefreshCcw size={17} />} action={button("Install", { action: "install" })} /> : null}
            {checks && checks.codex && !checks.codexVersion ? <GeneralPreferenceRow label="Agent version" detail="This installation requires Codex 0.153.3. Update it in Termux, then check again." icon={<Smartphone size={17} />} action={button("Open Termux", { action: "termux" })} /> : null}
            {checks && !checks.authenticated ? <GeneralPreferenceRow label="Agent sign-in" detail="The sign-in command will be copied. Paste it in Termux to continue." icon={<Smartphone size={17} />} action={button("Sign in", { action: "signin" })} /> : null}
            {checks && !needsTools && checks.authenticated && checks.codexVersion ? <InlineNotice message="Your phone is ready for local work." /> : null}
            <GeneralPreferenceRow label="Reconnect Termux" detail="Repair the saved connection without deleting projects or conversations." icon={<RefreshCcw size={17} />} action={button("Reconnect", { action: "repair" })} />
            <GeneralPreferenceRow label="Start after reboot" detail="Optional. Termux:Boot can start your workspace after a phone restart." icon={<Smartphone size={17} />} action={button(state.boot ? "Set up" : "Get Termux:Boot", { action: state.boot ? "boot" : "get_boot" })} />
          </> : null}
          <GeneralPreferenceRow label="Connection diagnostics" detail="Share connection and background-service events. Sign-in details are not included." icon={<Bug size={17} />} action={button("Share", { action: "diagnostics" })} />
        </div>
      </details>
      {state.notice ? <div role="status"><InlineNotice message={state.notice} /></div> : null}
      {error ? <p className="settings-notice" role="alert">{error}</p> : null}
    </div>
  );
}
