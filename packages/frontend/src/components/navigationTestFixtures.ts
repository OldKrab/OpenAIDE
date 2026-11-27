import type { AgentListedSession, TaskSummary } from "@openaide/app-shell-contracts";
import type { NativeSessionsState, NavigationGroupState, NavigationRowState } from "../state/store";

/** A Task row as App Server sends it; `live` mirrors the server's live-row flag. */
export function taskRow(task: TaskSummary, live = false): NavigationRowState {
  return { kind: "task", task, live };
}

/** A Native Session row as App Server sends it. */
export function sessionRow(session: AgentListedSession, live = false): NavigationRowState {
  return { kind: "session", session, live };
}

/**
 * A Navigation group exactly as the Sidebar receives it. Rows keep the given order, since the
 * server orders them; `total` defaults to the row count and `hidden` to zero.
 */
export function navigationGroup(
  projectId: string,
  label: string,
  rows: NavigationRowState[],
  overrides: Partial<Omit<NavigationGroupState, "projectId" | "label" | "rows">> = {},
): NavigationGroupState {
  return {
    projectId,
    label,
    rows,
    total: rows.length,
    hidden: 0,
    undiscovered: false,
    discovery: "settled",
    ...overrides,
  };
}

/** The Sidebar's `nativeSessions` prop for a view with these groups. */
export function navigationState(
  groups: NavigationGroupState[],
  overrides: Partial<NativeSessionsState> = {},
): NativeSessionsState {
  return {
    items: groups.flatMap((group) => group.rows.flatMap((row) => row.kind === "session" ? [row.session] : [])),
    groups,
    loaded: true,
    loading: false,
    ...overrides,
  };
}
