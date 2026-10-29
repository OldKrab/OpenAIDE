import { useCallback, useEffect, useState, type KeyboardEvent, type ReactNode } from "react";
import { ChevronDown, ChevronRight, File, Folder, FolderOpen } from "lucide-react";
import type { BackendConnection, ProjectFileEntry } from "@openaide/app-server-client";
import { ChangeCounts } from "./ProjectChangesView";
import { scopeKey, scopeParams, type FileScope } from "./projectFilesModel";
import { pageError, useProjectFileLoader, type FilePage } from "./useProjectFiles";

/** The whole workspace as a lazily read folder tree. Expanded folders are the owner's state so
 * they survive a switch to Changes and back; each folder is read when first opened. */
export function ProjectFilesTree({
  connection,
  scope,
  expanded,
  onExpanded,
  selected,
  changes,
  refresh,
  onOpen,
}: {
  connection: Pick<BackendConnection, "request">;
  scope: FileScope;
  expanded: Set<string>;
  onExpanded: (next: Set<string>) => void;
  /** Workspace-relative path of the file open in the reader. */
  selected?: string;
  /** Changed files, so the tree can show their counts. */
  changes: Map<string, ProjectFileEntry>;
  refresh: number;
  onOpen: (path: string) => void;
}) {
  const identity = scopeKey(scope);
  const { load } = useProjectFileLoader(connection, identity);
  const [directories, setDirectories] = useState<Record<string, FilePage>>({});
  const directory = useCallback(
    (path: string, cursor = 0) =>
      void load(`directory:${path}`, "files", scopeParams(scope, { path, cursor }), (next) =>
        setDirectories((old) => ({
          ...old,
          [path]: typeof next === "function" ? next(old[path] ?? { loading: false }) : next,
        })),
      ),
    // `scope` is a fresh object per render; its identity string is the dependency.
    [load, identity],
  );
  useEffect(() => {
    directory("");
    for (const path of expanded) directory(path);
    // Expanding a folder reads it directly; this re-reads what is open on workspace change and refresh.
  }, [directory, refresh]);

  function toggle(item: ProjectFileEntry) {
    const next = new Set(expanded);
    next.has(item.path) ? next.delete(item.path) : next.add(item.path);
    onExpanded(next);
    if (!directories[item.path]) directory(item.path);
  }
  function navigate(event: KeyboardEvent<HTMLButtonElement>, item: ProjectFileEntry) {
    const rows = [
      ...(event.currentTarget.closest("[role=tree]")?.querySelectorAll<HTMLButtonElement>("[role=treeitem]") ?? []),
    ];
    const index = rows.indexOf(event.currentTarget);
    const focus = (next: number) => rows[Math.max(0, Math.min(rows.length - 1, next))]?.focus();
    const open = item.directory && expanded.has(item.path);
    const moves: Record<string, () => void> = {
      ArrowDown: () => focus(index + 1),
      ArrowUp: () => focus(index - 1),
      Home: () => focus(0),
      End: () => focus(rows.length - 1),
      ArrowRight: () => (item.directory && !open ? toggle(item) : item.directory && focus(index + 1)),
      ArrowLeft: () =>
        open
          ? toggle(item)
          : rows.find((row) => row.dataset.path === item.path.split("/").slice(0, -1).join("/"))?.focus(),
    };
    const move = moves[event.key];
    if (!move) return;
    event.preventDefault();
    move();
  }
  function level(path: string, depth: number): ReactNode {
    const state = directories[path] ?? { loading: true };
    const failure = pageError(state);
    return (
      <>
        {failure && (
          <div className="project-files-error" role="alert">
            <span>{failure}</span>
            <button type="button" onClick={() => directory(path)}>
              Retry
            </button>
          </div>
        )}
        {/* Cached rows stay in place during a refresh; a loading row would shift the scroll position. */}
        {state.loading && !state.result && (
          <p role="status" className="project-files-note">
            Loading files…
          </p>
        )}
        {state.result?.entries.map((item) => {
          const open = item.directory && expanded.has(item.path);
          const change = item.directory ? undefined : changes.get(item.path);
          return (
            <div key={item.path}>
              <button
                type="button"
                className="project-files-row"
                role="treeitem"
                aria-level={depth + 1}
                aria-expanded={item.directory ? open : undefined}
                aria-current={!item.directory && selected === item.path ? "true" : undefined}
                data-path={item.path}
                style={{ paddingLeft: 10 + depth * 14 }}
                onKeyDown={(event) => navigate(event, item)}
                onClick={() => (item.directory ? toggle(item) : onOpen(item.path))}
              >
                {item.directory ? (
                  <>
                    {open ? (
                      <ChevronDown aria-hidden="true" size={13} className="project-files-chevron" />
                    ) : (
                      <ChevronRight aria-hidden="true" size={13} className="project-files-chevron" />
                    )}
                    {open ? <FolderOpen aria-hidden="true" size={14} /> : <Folder aria-hidden="true" size={14} />}
                  </>
                ) : (
                  <>
                    <span className="project-files-chevron" />
                    <File aria-hidden="true" size={14} />
                  </>
                )}
                <span className="project-files-row-label">{item.name}</span>
                {change && <ChangeCounts entry={change} />}
              </button>
              {open && level(item.path, depth + 1)}
            </div>
          );
        })}
        {state.result?.truncated && (
          <p className="project-files-note" role="status">
            This folder is too large to list completely.
          </p>
        )}
        {state.result?.nextCursor != null && (
          <button
            type="button"
            className="project-files-more"
            disabled={state.loading}
            onClick={() => directory(path, state.result?.nextCursor ?? 0)}
          >
            Load more
          </button>
        )}
      </>
    );
  }
  return (
    <div role="tree" aria-label="Workspace files">
      {level("", 0)}
    </div>
  );
}
