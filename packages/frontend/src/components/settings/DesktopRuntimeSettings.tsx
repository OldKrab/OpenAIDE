import { Check, Monitor, TerminalSquare } from "lucide-react";
import { useState } from "react";
import type {
  DesktopRuntimeCapability,
  DesktopRuntimeEnvironment,
} from "../../services/frontendShell";

export function DesktopRuntimeSettings({
  capability,
}: {
  capability: DesktopRuntimeCapability;
}) {
  const snapshot = capability.snapshot();
  const [choosingDistro, setChoosingDistro] = useState(false);
  const [pending, setPending] = useState<DesktopRuntimeEnvironment>();
  const [switching, setSwitching] = useState(false);
  const [error, setError] = useState<string>();
  const requestSwitch = (environment: DesktopRuntimeEnvironment) => {
    if (sameEnvironment(snapshot.active, environment)) return;
    setError(undefined);
    setPending(environment);
  };
  const chooseWsl = () => {
    if (snapshot.wslDistros.length === 1) {
      requestSwitch({ kind: "wsl", distro: snapshot.wslDistros[0] });
      return;
    }
    setChoosingDistro(true);
  };
  const confirmSwitch = async () => {
    if (!pending || switching) return;
    setSwitching(true);
    setError(undefined);
    try {
      await capability.select(pending);
    } catch {
      setError("OpenAIDE could not switch environments. Try again.");
      setSwitching(false);
    }
  };
  const pendingLabel = pending?.kind === "native" ? "Windows" : pending ? `WSL · ${pending.distro}` : "";
  return (
    <div className="desktop-runtime-settings">
      <div aria-label="OpenAIDE environment" className="desktop-runtime-choices" role="radiogroup">
        <button
          aria-label="Use Windows"
          aria-checked={snapshot.active.kind === "native"}
          className={snapshot.active.kind === "native" ? "selected" : ""}
          onClick={() => requestSwitch({ kind: "native" })}
          role="radio"
          type="button"
        >
          <span className="desktop-runtime-choice-icon"><Monitor size={17} /></span>
          <span className="desktop-runtime-choice-copy">
            <strong>Windows</strong>
            <small>Use Windows projects, tools, and Agents.</small>
          </span>
          {snapshot.active.kind === "native" ? <span className="desktop-runtime-active"><Check size={12} />Active</span> : null}
        </button>
        <button
          aria-label="Use WSL"
          aria-checked={snapshot.active.kind === "wsl"}
          className={snapshot.active.kind === "wsl" ? "selected" : ""}
          disabled={!snapshot.wslDistros.length}
          onClick={chooseWsl}
          role="radio"
          type="button"
        >
          <span className="desktop-runtime-choice-icon"><TerminalSquare size={17} /></span>
          <span className="desktop-runtime-choice-copy">
            <strong>WSL{snapshot.active.kind === "wsl" ? ` · ${snapshot.active.distro}` : ""}</strong>
            <small>Use Linux projects, tools, and Agents.</small>
          </span>
          {snapshot.active.kind === "wsl" ? <span className="desktop-runtime-active"><Check size={12} />Active</span> : null}
        </button>
      </div>
      {choosingDistro && snapshot.wslDistros.length > 1 ? (
        <label className="desktop-runtime-distro">
          <span>Distribution</span>
          <select
            aria-label="WSL distribution"
            defaultValue=""
            onChange={(event) => {
              if (event.currentTarget.value) {
                requestSwitch({ kind: "wsl", distro: event.currentTarget.value });
              }
            }}
          >
            <option disabled value="">Choose a distribution</option>
            {snapshot.wslDistros.map((distro) => <option key={distro}>{distro}</option>)}
          </select>
        </label>
      ) : null}
      {pending ? (
        <div aria-labelledby="desktop-runtime-confirm-title" className="desktop-runtime-confirm" role="alertdialog">
          <span>
            <strong id="desktop-runtime-confirm-title">Switch to {pendingLabel}?</strong>
            <small>OpenAIDE will restart. This environment has separate projects, Tasks, settings, Agents, and credentials.</small>
          </span>
          <span className="desktop-runtime-confirm-actions">
            <button disabled={switching} onClick={() => setPending(undefined)} type="button">Cancel</button>
            <button autoFocus className="primary" disabled={switching} onClick={() => void confirmSwitch()} type="button">
              {switching ? "Restarting…" : "Restart and switch"}
            </button>
          </span>
        </div>
      ) : null}
      {error ? <p className="desktop-runtime-error" role="alert">{error}</p> : null}
      {!snapshot.wslDistros.length ? (
        <p className="desktop-runtime-unavailable">Install an x64 WSL distribution to use OpenAIDE in Linux.</p>
      ) : null}
    </div>
  );
}

function sameEnvironment(left: DesktopRuntimeEnvironment, right: DesktopRuntimeEnvironment) {
  return left.kind === right.kind
    && (left.kind === "native" || (right.kind === "wsl" && left.distro === right.distro));
}
