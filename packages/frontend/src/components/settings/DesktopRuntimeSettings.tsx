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
  const [switching, setSwitching] = useState<DesktopRuntimeEnvironment>();
  const [error, setError] = useState<string>();
  const requestSwitch = async (environment: DesktopRuntimeEnvironment) => {
    if (sameEnvironment(snapshot.active, environment) || switching) return;
    setError(undefined);
    setSwitching(environment);
    try {
      await capability.select(environment);
    } catch {
      setError("OpenAIDE could not switch environments. Try again.");
      setSwitching(undefined);
    }
  };
  const chooseWsl = () => {
    if (snapshot.wslDistros.length === 1) {
      void requestSwitch({ kind: "wsl", distro: snapshot.wslDistros[0] });
      return;
    }
    setChoosingDistro(true);
  };
  return (
    <div className="desktop-runtime-settings">
      <div aria-label="OpenAIDE environment" className="desktop-runtime-choices" role="radiogroup">
        <button
          aria-label="Use Windows"
          aria-busy={switching?.kind === "native"}
          aria-checked={snapshot.active.kind === "native"}
          className={snapshot.active.kind === "native" ? "selected" : ""}
          disabled={Boolean(switching)}
          onClick={() => void requestSwitch({ kind: "native" })}
          role="radio"
          type="button"
        >
          <span className="desktop-runtime-choice-icon"><Monitor size={17} /></span>
          <span className="desktop-runtime-choice-copy">
            <strong>Windows</strong>
            <small>Use Windows projects, tools, and Agents.</small>
          </span>
          {snapshot.active.kind === "native" ? <span className="desktop-runtime-active"><Check size={12} />Active</span> : null}
          {switching?.kind === "native" ? <span className="desktop-runtime-active">Restarting…</span> : null}
        </button>
        <button
          aria-label="Use WSL"
          aria-busy={switching?.kind === "wsl"}
          aria-checked={snapshot.active.kind === "wsl"}
          className={snapshot.active.kind === "wsl" ? "selected" : ""}
          disabled={!snapshot.wslDistros.length || Boolean(switching)}
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
          {switching?.kind === "wsl" ? <span className="desktop-runtime-active">Restarting…</span> : null}
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
                void requestSwitch({ kind: "wsl", distro: event.currentTarget.value });
              }
            }}
          >
            <option disabled value="">Choose a distribution</option>
            {snapshot.wslDistros.map((distro) => <option key={distro}>{distro}</option>)}
          </select>
        </label>
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
