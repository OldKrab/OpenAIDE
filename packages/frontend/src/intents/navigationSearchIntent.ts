import {
  AppServerProtocolError,
  TASK_NAVIGATION_SEARCH,
  type BackendConnection,
  type ProjectId,
  type TaskNavigationSearchResult,
} from "@openaide/app-server-client";
import type { ClientDiagnosticFields } from "./clientDiagnosticsIntents";

type SearchConnection = Pick<BackendConnection, "request">;

export type NavigationSearchEvent =
  | "navigation_search_started"
  | "navigation_search_completed"
  | "navigation_search_failed";

/** Receives metadata only: lengths, counts, durations and error classes, never the search text. */
export type NavigationSearchLog = (event: NavigationSearchEvent, fields: ClientDiagnosticFields) => void;

/** Runs the one-shot server-side search (ADR-0086, "Search"). Results are not live. */
export async function requestNavigationSearch(
  connection: SearchConnection,
  text: string,
  log: NavigationSearchLog,
  projectIds?: readonly string[],
  now: () => number = () => performance.now(),
): Promise<TaskNavigationSearchResult> {
  const startedAt = now();
  const fields = { text_length: text.length, project_filter_count: projectIds?.length };
  log("navigation_search_started", fields);
  try {
    const result = await connection.request(TASK_NAVIGATION_SEARCH, {
      text,
      ...(projectIds ? { projectIds: [...projectIds] as ProjectId[] } : {}),
    });
    log("navigation_search_completed", {
      ...fields,
      duration_ms: now() - startedAt,
      hit_count: result.hits.length,
      truncated: result.truncated,
    });
    return result;
  } catch (error) {
    log("navigation_search_failed", {
      ...fields,
      duration_ms: now() - startedAt,
      error_name: error instanceof Error ? error.name : typeof error,
      error_code: error instanceof AppServerProtocolError ? error.protocolError.code : undefined,
    });
    throw error;
  }
}
