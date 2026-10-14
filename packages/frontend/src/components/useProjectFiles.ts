import { useCallback, useEffect, useRef, useState } from "react";
import type {
  BackendConnection,
  ProjectChangeBase,
  ProjectFilesParams,
  ProjectFilesResult,
} from "@openaide/app-server-client";
import { readProjectFiles, type ProjectFileOperation } from "../intents/projectFileIntents";
import { scopeKey, scopeParams, type FileScope } from "./projectFilesModel";

export type FilePage = { result?: ProjectFilesResult; loading: boolean; error?: string };
type Update = (next: FilePage | ((old: FilePage) => FilePage)) => void;
type Connection = Pick<BackendConnection, "request">;

/**
 * One reader of read-only file results. Each key keeps only its newest request: a response for
 * an earlier request, workspace or base is dropped, and a refresh keeps the rows it replaces on
 * screen until the new ones arrive so the list does not jump.
 */
export function useProjectFileLoader(connection: Connection | undefined, identity: string) {
  const request = connection?.request;
  const epochs = useRef(new Map<string, number>());
  useEffect(
    () => () => {
      epochs.current.forEach((value, key) => epochs.current.set(key, value + 1));
    },
    [request, identity],
  );
  const load = useCallback(
    async (key: string, operation: ProjectFileOperation, params: ProjectFilesParams, update: Update) => {
      if (!request) return;
      const epoch = (epochs.current.get(key) ?? 0) + 1;
      epochs.current.set(key, epoch);
      update((old) => ({ ...old, loading: true, error: undefined }));
      try {
        const result = await readProjectFiles({ request }, operation, params);
        if (epochs.current.get(key) !== epoch) return;
        update((old) => ({
          loading: false,
          result:
            params.cursor && old.result ? { ...result, entries: [...old.result.entries, ...result.entries] } : result,
        }));
      } catch (error) {
        if (epochs.current.get(key) === epoch)
          update({ loading: false, error: error instanceof Error ? error.message : "Unable to read project files." });
      }
    },
    [request],
  );
  const cancel = useCallback((key: string) => {
    epochs.current.set(key, (epochs.current.get(key) ?? 0) + 1);
  }, []);
  return { load, cancel };
}

export const fileErrors: Record<string, string> = {
  unavailable: "Files are unavailable. Try refreshing.",
  permissionDenied: "Permission denied.",
  outsideWorkspace: "This location is outside the workspace.",
  notRepository: "This workspace is not a Git repository.",
  noDefaultBranch: "No default branch was found to compare with.",
  timeout: "The operation timed out. Try a narrower search or refresh.",
  tooLarge: "This result is too large to display.",
  changed: "This file is no longer changed. Refresh Changes.",
};

export function pageError(page: FilePage) {
  return page.error ?? (page.result?.error ? (fileErrors[page.result.error] ?? "Unable to read files.") : undefined);
}

/**
 * The change list of one workspace against one base. It backs both the count on the Task header
 * and the Changes view, so the two can never disagree. `refresh` is bumped by the owner when the
 * panel opens, when the Agent finishes a turn, and on request.
 */
export function useProjectChanges({
  connection,
  enabled,
  scope,
  remembered,
  onBase,
  refresh,
}: {
  connection?: Connection;
  enabled: boolean;
  scope: FileScope;
  /** The base last chosen for this workspace on this device. */
  remembered?: ProjectChangeBase;
  onBase?: (base: ProjectChangeBase) => void;
  refresh: number;
}) {
  const identity = scopeKey(scope);
  const { load } = useProjectFileLoader(connection, identity);
  const [page, setPage] = useState<FilePage>({ loading: false });
  // Everything since the default branch is the useful first view of a worktree; the server
  // reports when that base does not exist or equals uncommitted work.
  const [requested, setRequested] = useState<ProjectChangeBase>(remembered ?? "defaultBranch");
  const owner = useRef(identity);
  if (owner.current !== identity) {
    owner.current = identity;
    setRequested(remembered ?? "defaultBranch");
    setPage({ loading: false });
  }
  const fetch = useCallback(
    (cursor = 0) => {
      if (enabled) void load("changes", "changes", scopeParams(scope, { base: requested, cursor }), setPage);
    },
    // `scope` is a fresh object per render; its identity string is the dependency.
    [enabled, load, identity, requested],
  );
  useEffect(() => fetch(), [fetch, refresh]);
  useEffect(() => {
    if (page.result?.error === "noDefaultBranch" && requested === "defaultBranch") setRequested("uncommitted");
  }, [page.result?.error, requested]);
  const result = page.result?.error === "noDefaultBranch" ? undefined : page.result;
  const defaultBranch = result?.defaultBranch ?? undefined;
  return {
    page: result === page.result ? page : { ...page, result },
    entries: result?.entries ?? [],
    /** Changed files before paging; the header count. */
    total: result?.total ?? result?.entries.length ?? 0,
    /** Present only when comparing with the default branch would differ from uncommitted work. */
    defaultBranch,
    base: (defaultBranch ? requested : "uncommitted") as ProjectChangeBase,
    /** The base sent with change and diff requests. */
    requestedBase: requested,
    repository: page.result?.error !== "notRepository",
    setBase: (base: ProjectChangeBase) => {
      setRequested(base);
      onBase?.(base);
    },
    loadMore: () => fetch(page.result?.nextCursor ?? 0),
  };
}
