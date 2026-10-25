import { computerName, currentComputer, type ComputersSnapshot } from "../services/computers";
import type { ConnectionProblem } from "../services/connectionProblem";

/**
 * The place the Frontend tried to use. A named computer comes from the shell's computers
 * capability; without one the App Server is unnamed, and `self` means it runs on this device.
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

/**
 * Says which place cannot be used, why in plain words, and what to do next. The raw error
 * and versions sit behind Details (DESIGN.md: troubleshooting behind an explicit affordance).
 * Shared by every shell; switching is offered only where the shell has other places.
 */
export function ConnectionProblemView({ message, onRetry, onUse, place, places = [], problem }: {
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
  return (
    <section
      aria-label="App Server connection error"
      aria-live="polite"
      className="task-surface task-loading app-server-error connection-problem"
    >
      <h2>{copy.title}</h2>
      <p>{copy.line}</p>
      {onRetry ? (
        <div className="connection-problem-actions">
          <button className="primary" onClick={onRetry} type="button">Try again</button>
        </div>
      ) : null}
      {onUse && places.length ? (
        <div aria-label="Use another place" className="connection-problem-places" role="group">
          <small>Or use</small>
          {places.map((other) => (
            <button key={other.id || "this-device"} onClick={() => onUse(other.id)} type="button">{other.label}</button>
          ))}
        </div>
      ) : null}
      <details className="connection-problem-details">
        <summary>Details</summary>
        <dl>
          {versions ? (
            <>
              <dt>This app</dt>
              <dd>Protocol {versions.clientProtocol}</dd>
              <dt>{placeLabel(place)}</dt>
              <dd>{versions.server ? `OpenAIDE ${versions.server.appVersion}, protocol ${versions.server.protocol}` : "Version unknown"}</dd>
            </>
          ) : null}
          <dt>Error</dt>
          <dd>{message}</dd>
        </dl>
      </details>
    </section>
  );
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
        ? { title: `Can’t reach ${name}`, line: "It is not answering. Restart OpenAIDE, then try again." }
        : { title: `Can’t reach ${name}`, line: `Make sure ${subject} is on, OpenAIDE is running there, and both devices are online.` };
    case "incompatible": {
      const running = problem.server ? ` runs OpenAIDE ${problem.server.appVersion}` : "";
      if (problem.update === "computer") {
        return { title: `${capitalize(subject)} needs an update`, line: `${capitalize(subject)}${running || " runs an older OpenAIDE"}, which is older than this app. Update OpenAIDE there, then try again.` };
      }
      if (problem.update === "app") {
        return { title: "This app needs an update", line: `${capitalize(subject)}${running || " runs a newer OpenAIDE"}, which is newer than this app. Update this app, then try again.` };
      }
      return { title: `${capitalize(subject)} runs a different version`, line: "This app and OpenAIDE there are different versions. Update both, then try again." };
    }
    default:
      return { title: `Can’t connect to ${name}`, line: "Try again. If this keeps happening, the details below say what failed." };
  }
}

function capitalize(text: string) {
  return text.charAt(0).toUpperCase() + text.slice(1);
}
