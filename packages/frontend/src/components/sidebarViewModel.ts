import type { NavigationGroupState, NavigationRowState } from "../state/store";
import { nativeSessionMeta, nativeSessionTitle } from "./taskSurfaceHelpers";

export interface SidebarViewModel {
  readonly emptyMessage: string;
  readonly visibleCount: number;
  /** The groups to render, in the server's order, each with the rows that match the search. */
  readonly visibleGroups: SidebarGroupView[];
}

/** A Navigation group with the rows to show; `rows` equals the group's rows when not searching. */
export type SidebarGroupView = {
  group: NavigationGroupState;
  rows: NavigationRowState[];
};

/**
 * Shapes the Sidebar from the server's Navigation groups. App Server owns the order, the window
 * and the counts; this only applies the search to the rows already loaded.
 */
export function sidebarViewModel({
  loading,
  nativeSessionAgentName,
  groups,
  searchQuery,
  showArchived,
}: {
  loading: boolean;
  nativeSessionAgentName: string;
  groups: NavigationGroupState[];
  searchQuery: string;
  showArchived: boolean;
}): SidebarViewModel {
  const query = searchQuery.trim().toLowerCase();
  // TODO(state-sync phase 7): Search is server-side; until then it filters the rows the view loaded.
  const matches = (row: NavigationRowState) => !query || (row.kind === "task"
    ? [row.task.title, row.task.agent_name, row.task.status]
    : [nativeSessionTitle(row.session), nativeSessionMeta(row.session, nativeSessionAgentName)]
  ).some((value) => value.toLowerCase().includes(query));
  const visibleGroups = groups
    .map((group): SidebarGroupView => ({ group, rows: group.rows.filter(matches) }))
    .filter(({ group, rows }) => !query || rows.length > 0 || group.label.toLowerCase().includes(query));
  const visibleCount = visibleGroups.reduce((count, view) => count + view.rows.length, 0);
  const emptyMessage = query
    ? showArchived
      ? "No archived items match."
      : "No matching tasks."
    : showArchived
      ? "Archive is empty. Archived Tasks and Native Sessions will appear here."
      : loading
        ? "Loading tasks."
        : "No tasks yet.";

  return { emptyMessage, visibleCount, visibleGroups };
}
