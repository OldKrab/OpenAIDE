import { useEffect, type ReactNode } from "react";
import { computerName, currentComputer, type ComputersSnapshot } from "../services/computers";
import { useComputerSwitch, type ComputerSwitch, type ComputerSwitchState } from "../services/computerSwitch";
import type { ConnectionProblem } from "../services/connectionProblem";
import { ConnectionPage, thisDevice } from "./ConnectionPage";
import { ConnectionProblemView, connectionPlaceFor } from "./ConnectionProblemView";

/**
 * A switch in progress or refused (ADR-0070) takes the main surface, so the wait and its
 * outcome are a page, not a popup row. The computer in use keeps running underneath until
 * the new one answers; Cancel and Back return to it.
 */
export function ComputerSwitchPage({ snapshot, state, switcher }: {
  snapshot: ComputersSnapshot | undefined;
  state: ComputerSwitchState;
  switcher: ComputerSwitch;
}) {
  const target = state.pending ?? state.failed?.id ?? "";
  const name = targetName(snapshot, target);
  const inUse = snapshot ? currentComputer(snapshot) : undefined;
  const here = inUse ? computerName(inUse) : snapshot?.device === "phone" ? "this phone" : "this computer";
  const ends = {
    here: thisDevice(snapshot?.device),
    there: { label: capitalize(name), kind: target ? "computer" : snapshot?.device ?? "computer" } as const,
  };
  if (state.failed) {
    return (
      <ConnectionPage
        {...ends}
        actions={(
          <>
            <button className="connection-primary" onClick={() => void switcher.request(target || undefined).catch(() => undefined)} type="button">Try again</button>
            <button className="connection-secondary" onClick={() => switcher.dismiss()} type="button">Back to {here}</button>
          </>
        )}
        label="Switching computers"
        line={state.failed.message}
        link="broken"
        title={`Can’t switch to ${name}`}
      />
    );
  }
  return (
    <ConnectionPage
      {...ends}
      actions={state.cancellable ? <button className="connection-secondary" onClick={() => switcher.cancel()} type="button">Cancel</button> : undefined}
      busy
      label="Switching computers"
      line={`You stay on ${here} until it answers.`}
      link="waiting"
      title={`Connecting to ${name}`}
    />
  );
}

/**
 * The web workbench's main surface while no Task can be shown: a switch, then a connection
 * problem, then the Task surface. It also closes the phone's navigation drawer once a switch
 * starts, so the page is what the user sees.
 */
export function WorkbenchConnectionSurface({ appServerError, appServerProblem, children, closeNavigation, onRetry, serverName, snapshot, switcher }: {
  appServerError?: string;
  appServerProblem?: ConnectionProblem;
  children: ReactNode;
  closeNavigation: () => void;
  onRetry: () => void;
  serverName?: string;
  snapshot: ComputersSnapshot | undefined;
  switcher: ComputerSwitch | undefined;
}) {
  const state = useComputerSwitch(switcher);
  const switching = state.pending !== undefined || state.failed !== undefined;
  useEffect(() => {
    if (state.pending !== undefined) closeNavigation();
  }, [state.pending, closeNavigation]);
  if (switching && switcher) return <ComputerSwitchPage snapshot={snapshot} state={state} switcher={switcher} />;
  if (!appServerError) return <>{children}</>;
  const others = switcher && snapshot ? otherPlaces(snapshot) : [];
  return (
    <ConnectionProblemView
      device={snapshot?.device}
      message={appServerError}
      onRetry={onRetry}
      onUse={others.length && switcher ? (id) => void switcher.request(id || undefined).catch(() => undefined) : undefined}
      place={connectionPlaceFor(snapshot, serverName)}
      places={others}
      problem={appServerProblem}
    />
  );
}

/**
 * The phone header's title while this surface covers the Task, so the bar does not name a
 * Task that is not on screen; undefined while the Task shows.
 */
export function connectionSurfaceTitle(state: ComputerSwitchState, appServerError: string | undefined) {
  if (state.pending !== undefined || state.failed !== undefined) return "Switching computers";
  return appServerError ? "Not connected" : undefined;
}

/** Every place except the one in use; "" is this device. */
function otherPlaces(snapshot: ComputersSnapshot) {
  const inUse = currentComputer(snapshot);
  const here = { id: "", label: snapshot.device === "phone" ? "This phone" : "This computer" };
  return [
    ...(inUse ? [here] : []),
    ...snapshot.computers.filter((computer) => !computer.current).map((computer) => ({ id: computer.id, label: computerName(computer) })),
  ];
}

function targetName(snapshot: ComputersSnapshot | undefined, id: string) {
  if (!id) return snapshot?.device === "phone" ? "this phone" : "this computer";
  const computer = snapshot?.computers.find((candidate) => candidate.id === id);
  return computer ? computerName(computer) : "the computer";
}

function capitalize(text: string) {
  return text.charAt(0).toUpperCase() + text.slice(1);
}
