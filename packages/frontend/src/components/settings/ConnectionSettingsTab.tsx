import { Battery, Bell, Bug, Laptop, RefreshCcw, Smartphone } from "lucide-react";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { ConnectionCommand, ConnectionSettings } from "../../services/connectionSettings";
import { GeneralPreferenceRow, GeneralSection, SettingsSwitch } from "./GeneralSettingsTab";
import { InlineNotice, SettingsSkeleton } from "./settingsPresentation";
import "./ConnectionSettingsTab.css";

export function ConnectionSettingsTab({ capability }: { capability: ConnectionSettings }) {
  const state = useSyncExternalStore(capability.subscribe, capability.snapshot, capability.snapshot);
  const [destination, setDestination] = useState<"local" | "remote">("local");
  const [address, setAddress] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [sending, setSending] = useState(false);
  const initialized = useRef(false);
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
  useEffect(() => {
    if (!state || initialized.current) return;
    initialized.current = true;
    setDestination(state.remote ? "remote" : "local");
    setAddress(state.address);
    setUsername(state.username);
  }, [state]);
  useEffect(() => {
    if (state?.scannedAddress) setAddress(state.scannedAddress);
  }, [state?.scanSequence]);

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
      <GeneralSection id="settings-connection-workspace" label="Workspace" description="Choose where agents run. Your projects and conversations stay on that device.">
        <div className="desktop-runtime-settings">
          <div className="desktop-runtime-choices connection-runtime-choices" role="radiogroup" aria-label="Where agents run" onKeyDown={event => {
            if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(event.key) || busy) return;
            event.preventDefault();
            const next = event.key === "Home" ? "local" : event.key === "End" ? "remote" : destination === "local" ? "remote" : "local";
            setDestination(next);
            event.currentTarget.querySelectorAll<HTMLButtonElement>("button")[next === "local" ? 0 : 1]?.focus();
          }}>
            {(["local", "remote"] as const).map(choice => (
              <button key={choice} type="button" role="radio" aria-checked={destination === choice} tabIndex={destination === choice ? 0 : -1} className={destination === choice ? "selected" : ""} disabled={busy} onClick={() => setDestination(choice)}>
                <span className="desktop-runtime-choice-icon">{choice === "local" ? <Smartphone size={17} /> : <Laptop size={17} />}</span>
                <span className="desktop-runtime-choice-copy"><strong>{choice === "local" ? "This phone" : "Remote computer"}</strong><small>{choice === "local" ? "Run agents in Termux" : "Connect to your server"}</small></span>
                {state.remote === (choice === "remote") ? <span className="desktop-runtime-active">Connected</span> : null}
              </button>
            ))}
          </div>
          {destination === "local" ? (
            <>
              <InlineNotice message={state.remote ? "Switch to the projects and conversations stored on this phone. Remote work is not stopped." : "OpenAIDE starts Termux when needed. No manual launch required."} />
              {state.remote ? <div>{button("Connect to this phone", { action: "local" })}</div> : null}
            </>
          ) : (
            <div className="general-preference-surface">
            <form className="agent-sign-in-fields" onSubmit={event => { event.preventDefault(); void execute({ action: "remote", address, username, password }); }}>
              <label><span>Server address</span><input type="url" autoComplete="url" placeholder="https://your-computer.example" required value={address} disabled={busy} onChange={event => setAddress(event.currentTarget.value)} /></label>
              <label><span>Username</span><input autoComplete="username" required value={username} disabled={busy} onChange={event => setUsername(event.currentTarget.value)} /></label>
              <label><span>Password</span><input type="password" autoComplete="current-password" required value={password} disabled={busy} onChange={event => setPassword(event.currentTarget.value)} /></label>
              <div className="agent-sign-in-value-actions">
                {button("Read QR image", { action: "qr" })}
                <button type="submit" className="agent-page-row-button primary" disabled={busy}>Connect to computer</button>
              </div>
              <InlineNotice message="Use your OpenAIDE server’s HTTPS address and sign-in details. The current connection stays unchanged until these are verified." />
            </form>
            </div>
          )}
        </div>
      </GeneralSection>

      {!state.remote && destination === "local" ? (
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
      ) : state.remote && destination === "remote" ? <InlineNotice message="Remote work continues on your computer when you close OpenAIDE or lock your phone. Keep that computer awake and connected." /> : null}

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
