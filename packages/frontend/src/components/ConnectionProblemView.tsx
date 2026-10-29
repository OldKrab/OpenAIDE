import { ChevronDown, ChevronRight, Monitor, MonitorSmartphone, Smartphone } from "lucide-react";
import { computerName, currentComputer, type ComputersSnapshot } from "../services/computers";
import type { ConnectionProblem } from "../services/connectionProblem";
import { ConnectionPage, thisDevice, type ConnectionEnd } from "./ConnectionPage";

/**
 * The place the Frontend tried to use. A named computer comes from the shell's computers
 * capability; without one the App Server is unnamed, and `self` means it runs on this device.
 *
 * TODO: rename to ConnectionComputer (and `place`/`places` props to `computer`/`computers`):
 * CONTEXT.md names this a Computer, and Place means the Task or Project last open on one.
 */
export type ConnectionPlace = { name: string; self?: "phone" | "computer" };

/** Names the place in use: the shell's current computer, this device, or the App Server's own name. */
export function connectionPlaceFor(computers: ComputersSnapshot | undefined, serverName: string | undefined): ConnectionPlace | undefined {
  const inUse = computers ? currentComputer(computers) : undefined;
  if (inUse) return { name: computerName(inUse) };
  if (computers) return { name: computers.device === "phone" ? "This phone" : "This computer", self: computers.device };
  return serverName ? { name: serverName } : undefined;
}

type Copy = { title: string; line: string };
type Incompatible = Extract<ConnectionProblem, { kind: "incompatible" }>;

/**
 * Says which place cannot be used, why in a few words, and what to do next. The raw error
 * and protocol versions sit behind Details (DESIGN.md: troubleshooting behind an explicit
 * affordance). Shared by every shell; switching is offered only where the shell has other places.
 */
export function ConnectionProblemView({ device, message, onRetry, onUse, place, places = [], problem }: {
  /** The device this Frontend runs on; a computer when the shell does not say. */
  device?: "phone" | "computer";
  message: string;
  onRetry?: () => void;
  /** Switches to one of `places`; "" is this device. */
  onUse?: (id: string) => void;
  place?: ConnectionPlace;
  /** The other places agents can work, offered right here instead of in the navigation. */
  places?: { id: string; label: string }[];
  problem?: ConnectionProblem;
}) {
  const copy = problemCopy(problem, place);
  const versions = problem?.kind === "incompatible" ? problem : undefined;
  const [here, there] = linkEnds(device, place, versions);
  return (
    <ConnectionPage
      actions={onRetry ? <button className="connection-primary" onClick={onRetry} type="button">Try again</button> : undefined}
      here={here}
      label="App Server connection error"
      line={copy.line}
      link={versions ? "mismatch" : "broken"}
      there={there}
      title={copy.title}
    >
      {onUse && places.length ? (
        <div className="connection-places">
          <span className="connection-places-title">Switch to</span>
          <div aria-label="Switch to" className="connection-places-list" role="group">
            {places.map((other) => {
              const Icon = other.id ? MonitorSmartphone : device === "phone" ? Smartphone : Monitor;
              return (
                <button key={other.id || "this-device"} onClick={() => onUse(other.id)} type="button">
                  <Icon aria-hidden="true" size={16} strokeWidth={1.6} />
                  <span>{other.label}</span>
                  <ChevronRight aria-hidden="true" size={16} />
                </button>
              );
            })}
          </div>
        </div>
      ) : null}
      <details className="connection-details">
        <summary>Details <ChevronDown aria-hidden="true" size={14} /></summary>
        <dl>
          {versions ? (
            <>
              <div><dt>This app</dt><dd>Protocol {versions.clientProtocol}</dd></div>
              <div>
                <dt>{placeLabel(place)}</dt>
                <dd>{versions.server ? `OpenAIDE ${versions.server.appVersion} · protocol ${versions.server.protocol}` : "Version unknown"}</dd>
              </div>
            </>
          ) : null}
          <div><dt>Error</dt><dd>{message}</dd></div>
        </dl>
      </details>
    </ConnectionPage>
  );
}

/**
 * The picture's two ends. A version mismatch marks the side to update; the computer's app
 * version shows when the probe read it.
 * TODO: show this app's version on the near end too. No shell passes the Frontend its own
 * app version yet, and setting this app's protocol beside the computer's app version would mislead.
 */
function linkEnds(device: "phone" | "computer" | undefined, place: ConnectionPlace | undefined, versions: Incompatible | undefined): [ConnectionEnd, ConnectionEnd] {
  const here: ConnectionEnd = place?.self ? { label: "This app", kind: "app" } : thisDevice(device);
  const there: ConnectionEnd = { label: placeLabel(place), kind: place?.self ?? "computer" };
  if (versions?.update === "app") Object.assign(here, { note: "Update needed", stale: true });
  if (versions?.server) there.note = versions.server.appVersion;
  if (versions?.update === "computer") Object.assign(there, { note: there.note ?? "Update needed", stale: true });
  return [here, there];
}

function placeLabel(place: ConnectionPlace | undefined) {
  if (place?.self) return place.self === "phone" ? "This phone" : "This computer";
  return place?.name ?? "App Server";
}

/** Exported for tests: the copy is the contract with the user. */
export function problemCopy(problem: ConnectionProblem | undefined, place: ConnectionPlace | undefined): Copy {
  // "OLD-PC" when known; otherwise the App Server on this device or an unnamed one.
  const name = place?.self ? `OpenAIDE on this ${place.self}` : place?.name ?? "the App Server";
  const subject = place?.self ? `this ${place.self}` : place?.name ?? "the computer";
  switch (problem?.kind) {
    case "unreachable":
      return place?.self
        ? { title: `Can’t reach ${name}`, line: "Restart OpenAIDE, then try again." }
        : { title: `Can’t reach ${name}`, line: "Check that it’s on and OpenAIDE is running there." };
    case "incompatible":
      if (problem.update === "computer") return { title: `${capitalize(subject)} needs an update`, line: `Update OpenAIDE on ${subject}, then try again.` };
      if (problem.update === "app") return { title: "This app needs an update", line: `${capitalize(subject)} runs a newer OpenAIDE. Update this app, then try again.` };
      return { title: `${capitalize(subject)} runs a different version`, line: "Update OpenAIDE here and there, then try again." };
    default:
      return { title: `Can’t connect to ${name}`, line: "Try again. Details say what failed." };
  }
}

function capitalize(text: string) {
  return text.charAt(0).toUpperCase() + text.slice(1);
}
