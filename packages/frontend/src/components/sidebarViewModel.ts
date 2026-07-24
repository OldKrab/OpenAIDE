import type { AgentListedSession } from "@openaide/app-shell-contracts";
import { nativeSessionMeta, nativeSessionTitle } from "./taskSurfaceHelpers";

export interface SidebarViewModel {
  readonly emptyMessage: string;
  readonly visibleCount: number;
  readonly visibleNativeSessions: AgentListedSession[];
}

export interface SidebarNativeSessionListState {
  readonly items: AgentListedSession[];
  readonly loading: boolean;
}

export function sidebarViewModel({
  loadingTasks,
  nativeSessionAgentName,
  nativeSessions,
  searchQuery,
  showArchived,
  taskCount,
}: {
  loadingTasks?: boolean;
  nativeSessionAgentName: string;
  nativeSessions: SidebarNativeSessionListState;
  searchQuery: string;
  showArchived: boolean;
  taskCount: number;
}): SidebarViewModel {
  const hasSearch = searchQuery.trim().length > 0;
  const query = searchQuery.trim().toLowerCase();
  const visibleNativeSessions =
    !query
      ? nativeSessions.items
      : nativeSessions.items.filter((session) =>
          [nativeSessionTitle(session), nativeSessionMeta(session, nativeSessionAgentName)].some((value) =>
            value.toLowerCase().includes(query),
          ),
        );
  const visibleCount = taskCount + visibleNativeSessions.length;
  const emptyMessage = hasSearch
    ? showArchived
      ? "No archived items match."
      : "No matching tasks."
    : showArchived
      ? "Archive is empty. Archived Tasks and Native Sessions will appear here."
      : loadingTasks || nativeSessions.loading
        ? "Loading tasks."
        : "No tasks yet.";

  return { emptyMessage, visibleCount, visibleNativeSessions };
}
