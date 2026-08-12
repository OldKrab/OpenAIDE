import { useState } from "react";
import type {
  DesktopRuntimeCapability,
  DesktopRuntimeEnvironment,
} from "../../services/frontendShell";

export function DesktopRuntimeSettings({
  capability,
  confirm = (message) => window.confirm(message),
}: {
  capability: DesktopRuntimeCapability;
  confirm?: (message: string) => boolean;
}) {
  const snapshot = capability.snapshot();
  const [choosingDistro, setChoosingDistro] = useState(false);
  const switchTo = (environment: DesktopRuntimeEnvironment) => {
    if (sameEnvironment(snapshot.active, environment)) return;
    const label = environment.kind === "native" ? "Windows" : `WSL: ${environment.distro}`;
    if (!confirm(`Restart OpenAIDE and switch to ${label}?`)) return;
    void capability.select(environment);
  };
  const chooseWsl = () => {
    if (snapshot.wslDistros.length === 1) {
      switchTo({ kind: "wsl", distro: snapshot.wslDistros[0] });
      return;
    }
    setChoosingDistro(true);
  };
  return (
    <div className="desktop-runtime-settings">
      <div aria-label="OpenAIDE environment" className="desktop-runtime-choices" role="radiogroup">
        <button
          aria-checked={snapshot.active.kind === "native"}
          className={snapshot.active.kind === "native" ? "selected" : ""}
          onClick={() => switchTo({ kind: "native" })}
          role="radio"
          type="button"
        >Windows</button>
        <button
          aria-checked={snapshot.active.kind === "wsl"}
          className={snapshot.active.kind === "wsl" ? "selected" : ""}
          disabled={!snapshot.wslDistros.length}
          onClick={chooseWsl}
          role="radio"
          type="button"
        >WSL</button>
      </div>
      {choosingDistro && snapshot.wslDistros.length > 1 ? (
        <label className="desktop-runtime-distro">
          <span>Distribution</span>
          <select
            aria-label="WSL distribution"
            defaultValue=""
            onChange={(event) => {
              if (event.currentTarget.value) {
                switchTo({ kind: "wsl", distro: event.currentTarget.value });
              }
            }}
          >
            <option disabled value="">Choose a distribution</option>
            {snapshot.wslDistros.map((distro) => <option key={distro}>{distro}</option>)}
          </select>
        </label>
      ) : null}
      {!snapshot.wslDistros.length ? (
        <p className="desktop-runtime-unavailable">Install an x64 WSL distribution to use OpenAIDE in Linux.</p>
      ) : (
        <p>Windows and each WSL distribution keep separate projects, Tasks, settings, Agents, and credentials.</p>
      )}
    </div>
  );
}

function sameEnvironment(left: DesktopRuntimeEnvironment, right: DesktopRuntimeEnvironment) {
  return left.kind === right.kind
    && (left.kind === "native" || (right.kind === "wsl" && left.distro === right.distro));
}
