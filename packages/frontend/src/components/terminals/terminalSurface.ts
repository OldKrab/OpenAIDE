import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import type { TerminalSummary } from "@openaide/app-server-client";
import { currentFrontendShell } from "../../services/frontendShell";

/**
 * What the Terminal page shows: one Project's Terminals. New ones start in the Task Workspace
 * when the page was opened from a Task, otherwise in the Project root.
 */
export type TerminalPageTarget = { projectId: string; taskId?: string };

export type TerminalSurface = {
  /** Every Terminal of the App Server, for counts; absent until the baseline arrives. */
  terminals?: TerminalSummary[];
  openPage(target: TerminalPageTarget): void;
};

/**
 * Present only in shells that host the in-app Terminal page. Entry points render nothing
 * without it, so the editor shell keeps its own native terminal.
 */
export const TerminalSurfaceContext = createContext<TerminalSurface | undefined>(undefined);

export function useTerminalSurface() {
  return useContext(TerminalSurfaceContext);
}

/** Terminals that have not exited, which is what quitting or replacing the App Server closes. */
export function openTerminalCount(terminals: TerminalSummary[] | undefined, projectId?: string) {
  return (terminals ?? []).filter((terminal) =>
    terminal.status !== "exited" && (projectId === undefined || terminal.projectId === projectId)).length;
}

/**
 * Ephemeral page placement owned by the workbench. Leaving the route that opened the page
 * returns to it, so the Terminal page never outlives the Task or surface it came from.
 */
export function useTerminalPage(routeKey: string, terminals: TerminalSummary[] | undefined, onOpened?: () => void) {
  const [page, setPage] = useState<{ target: TerminalPageTarget; routeKey: string }>();
  const opened = useRef(onOpened);
  opened.current = onOpened;
  const openPage = useCallback((target: TerminalPageTarget) => {
    setPage({ target, routeKey });
    opened.current?.();
  }, [routeKey]);
  const closePage = useCallback(() => setPage(undefined), []);
  useEffect(() => {
    if (page && page.routeKey !== routeKey) setPage(undefined);
  }, [page, routeKey]);
  const openCount = openTerminalCount(terminals);
  useEffect(() => {
    currentFrontendShell()?.terminals?.openCountChanged?.(openCount);
  }, [openCount]);
  const surface = useMemo<TerminalSurface>(() => ({ terminals, openPage }), [terminals, openPage]);
  return { target: page?.routeKey === routeKey ? page.target : undefined, closePage, surface };
}
