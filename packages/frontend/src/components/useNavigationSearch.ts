import { useCallback, useEffect, useState } from "react";
import type { TaskNavigationSearchResult } from "@openaide/app-server-client";
import { useCurrentCallback } from "./useCurrentCallback";

/** Typing pauses this long before a request goes out. */
export const NAVIGATION_SEARCH_DEBOUNCE_MS = 220;

export type NavigationSearchState =
  | { status: "idle" }
  /** `previous` keeps the last answer on screen while the next one is on its way. */
  | { status: "loading"; previous?: TaskNavigationSearchResult }
  | { status: "ready"; result: TaskNavigationSearchResult }
  | { status: "error"; message: string };

export type RunNavigationSearch = (text: string) => Promise<TaskNavigationSearchResult>;

/**
 * Debounced one-shot search for the text in the sidebar field. A reply to an older text, or
 * one that lands after the field was cleared, is ignored: only the latest text may answer.
 */
export function useNavigationSearch(
  text: string,
  run: RunNavigationSearch | undefined,
  debounceMs = NAVIGATION_SEARCH_DEBOUNCE_MS,
) {
  const query = text.trim();
  const [state, setState] = useState<NavigationSearchState>({ status: "idle" });
  const [attempt, setAttempt] = useState(0);
  const latestRun = useCurrentCallback((value: string) => run!(value));
  const enabled = run !== undefined && query.length > 0;
  useEffect(() => {
    if (!enabled) {
      setState({ status: "idle" });
      return undefined;
    }
    let current = true;
    setState((previous) => ({
      status: "loading",
      previous: previous.status === "ready" ? previous.result
        : previous.status === "loading" ? previous.previous : undefined,
    }));
    const timer = setTimeout(() => {
      latestRun(query).then(
        (result) => { if (current) setState({ status: "ready", result }); },
        (error: unknown) => {
          if (!current) return;
          setState({ status: "error", message: error instanceof Error ? error.message : "Search failed." });
        },
      );
    }, debounceMs);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [attempt, debounceMs, enabled, latestRun, query]);
  const retry = useCallback(() => setAttempt((value) => value + 1), []);
  return { state, retry };
}
