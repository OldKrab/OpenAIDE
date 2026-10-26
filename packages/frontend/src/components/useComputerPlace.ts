import { useEffect, useRef, useState } from "react";
import { currentComputer, type ComputersSnapshot } from "../services/computers";
import { reportComputerSwitchArrival } from "../services/computerSwitch";
import type { WebviewBootstrap } from "../state/surfaceTypes";

/** Where the user was on one computer. Settings is never a place to return to. */
export type ComputerPlace = { kind: "task"; taskId: string } | { kind: "newTask"; projectId?: string };

const PLACE_KEY_PREFIX = "openaide.computerPlace:";

/**
 * Remembers, per computer, the last Task or New Task Project, and returns there
 * when the page opens on that computer at the start page (ADR-0070). Presentation
 * state on this device only. It stays off until another computer is paired, so a
 * Desktop that never pairs keeps opening at the start page.
 *
 * Returns true while the page may still return to a Task: the start page then shows
 * the Task opening rather than flashing New Task before the jump.
 */
export function useComputerPlace({ available, backendReady, bootstrap, hasProject, openNewTask, openTask, snapshot, storage = globalStorage() }: {
  /** The shell has a computers capability; without one there is no place to return to. */
  available: boolean;
  backendReady: boolean;
  bootstrap: WebviewBootstrap;
  hasProject: (projectId: string) => boolean;
  openNewTask: (projectId?: string) => void;
  openTask: (taskId: string) => void;
  snapshot: ComputersSnapshot | undefined;
  storage?: Storage;
}) {
  const computer = snapshot?.computers.length ? currentComputer(snapshot)?.id ?? "" : undefined;
  const openedAt = useRef(bootstrap);
  const mountedAt = useRef(performance.now());
  const settled = useRef(false);
  // Mirrors `settled` so the start page re-renders when a restore ends without navigating.
  const [restoreSettled, setRestoreSettled] = useState(false);
  const latest = useRef({ hasProject, openNewTask, openTask });
  latest.current = { hasProject, openNewTask, openTask };

  useEffect(() => {
    // The shell may list its computers after the App Server is ready; the place depends on both.
    if (!backendReady || settled.current || !snapshot) return;
    settled.current = true;
    setRestoreSettled(true);
    reportComputerSwitchArrival();
    if (computer === undefined) return;
    // A route chosen by the shell or the user before the App Server was ready wins.
    if (bootstrap !== openedAt.current || !isStartPage(bootstrap)) return;
    const place = readPlace(storage, computer);
    if (place?.kind === "task") latest.current.openTask(place.taskId);
    else if (place?.projectId && latest.current.hasProject(place.projectId)) latest.current.openNewTask(place.projectId);
    // A Task that no longer exists falls back to the start page through the Task route itself.
    // `waited_ms` is how long the start page held "Opening task" for the App Server and the computers.
    console.info(`computer_place_restore outcome=${place ? place.kind : "none"} waited_ms=${Math.round(performance.now() - mountedAt.current)}`);
  }, [backendReady, bootstrap, computer, snapshot, storage]);

  useEffect(() => {
    if (!settled.current || computer === undefined) return;
    const place = placeOf(bootstrap);
    if (place) writePlace(storage, computer, place);
  }, [bootstrap, computer, storage]);

  if (!available || restoreSettled || bootstrap !== openedAt.current || !isStartPage(bootstrap)) return false;
  // Until the shell lists its computers the computer in use is unknown; a device that ever
  // saved a Task place has paired, so it waits rather than flash New Task.
  if (!snapshot) return hasTaskPlace(storage);
  return computer !== undefined && readPlace(storage, computer)?.kind === "task";
}

function isStartPage(bootstrap: WebviewBootstrap) {
  return bootstrap.surface === "task" && !bootstrap.taskId && !bootstrap.projectId;
}

function placeOf(bootstrap: WebviewBootstrap): ComputerPlace | undefined {
  if (bootstrap.surface !== "task") return undefined;
  return bootstrap.taskId ? { kind: "task", taskId: bootstrap.taskId } : { kind: "newTask", projectId: bootstrap.projectId };
}

function readPlace(storage: Storage | undefined, computer: string): ComputerPlace | undefined {
  try {
    const value = JSON.parse(storage?.getItem(PLACE_KEY_PREFIX + computer) ?? "null") as Partial<{ kind: string; taskId: unknown; projectId: unknown }> | null;
    if (value?.kind === "task" && typeof value.taskId === "string") return { kind: "task", taskId: value.taskId };
    if (value?.kind === "newTask") return { kind: "newTask", projectId: typeof value.projectId === "string" ? value.projectId : undefined };
  } catch {
    // An unreadable place is no place: the start page opens.
  }
  return undefined;
}

function hasTaskPlace(storage: Storage | undefined) {
  try {
    for (let index = 0; index < (storage?.length ?? 0); index++) {
      const key = storage?.key(index);
      if (key?.startsWith(PLACE_KEY_PREFIX) && readPlace(storage, key.slice(PLACE_KEY_PREFIX.length))?.kind === "task") return true;
    }
  } catch {
    // Blocked storage holds no place.
  }
  return false;
}

function writePlace(storage: Storage | undefined, computer: string, place: ComputerPlace) {
  try {
    storage?.setItem(PLACE_KEY_PREFIX + computer, JSON.stringify(place));
  } catch {
    // Full or blocked storage only costs the return to this place.
  }
}

function globalStorage(): Storage | undefined {
  try {
    return typeof localStorage === "undefined" ? undefined : localStorage;
  } catch {
    return undefined;
  }
}
