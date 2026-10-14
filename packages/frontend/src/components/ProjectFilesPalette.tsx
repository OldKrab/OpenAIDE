import { useEffect, useRef, useState } from "react";
import { File, X } from "lucide-react";
import type { BackendConnection } from "@openaide/app-server-client";
import { scopeKey, scopeParams, type FileScope } from "./projectFilesModel";
import { pageError, useProjectFileLoader, type FilePage } from "./useProjectFiles";
import { useBackNavigation } from "./useBackNavigation";
import "../styles/app/project-files-palette.css";

export type FileSearchMode = "files" | "text";

/** Find a file by name or search file contents in one workspace. A palette over the panel on a
 * wide window and the whole screen on a phone; it only picks a file, the owner opens it. */
export function ProjectFilesPalette({
  connection,
  scope,
  workspaceLabel,
  mode,
  onMode,
  onClose,
  onOpen,
}: {
  connection: Pick<BackendConnection, "request">;
  scope: FileScope;
  workspaceLabel: string;
  mode: FileSearchMode;
  onMode: (mode: FileSearchMode) => void;
  onClose: () => void;
  onOpen: (path: string, line?: number) => void;
}) {
  const identity = scopeKey(scope);
  const { load, cancel } = useProjectFileLoader(connection, identity);
  const [query, setQuery] = useState("");
  const [matchCase, setMatchCase] = useState(false);
  const [includeIgnored, setIncludeIgnored] = useState(false);
  const [page, setPage] = useState<FilePage>({ loading: false });
  const input = useRef<HTMLInputElement>(null);
  useBackNavigation(true, onClose, 120);
  const search = (cursor = 0) =>
    void load(
      "search",
      "search",
      scopeParams(scope, { query, content: mode === "text", caseSensitive: matchCase, includeIgnored, cursor }),
      setPage,
    );
  useEffect(() => {
    cancel("search");
    setPage({ loading: false });
    if (!query.trim()) return;
    // Typing pauses before a scan starts; a newer query supersedes the one in flight.
    const timer = setTimeout(() => search(), 180);
    return () => clearTimeout(timer);
  }, [query, mode, matchCase, includeIgnored, identity]);
  useEffect(() => input.current?.focus(), [mode]);
  const entries = page.result?.entries ?? [];
  const failure = pageError(page);
  const choose = (index: number) => {
    const entry = entries[index];
    if (entry) onOpen(entry.path, entry.line ?? undefined);
  };
  return (
    <div
      className="project-files-palette-backdrop"
      onPointerDown={(event) => event.target === event.currentTarget && onClose()}
    >
      <div
        className="project-files-palette"
        role="dialog"
        aria-label="Search files"
        onKeyDown={(event) => {
          if (event.key !== "Escape") return;
          event.preventDefault();
          event.stopPropagation();
          onClose();
        }}
      >
        <div className="project-files-palette-head">
          <div className="project-files-segment" role="group" aria-label="Search in">
            <button type="button" aria-pressed={mode === "files"} onClick={() => onMode("files")}>
              Files
            </button>
            <button type="button" aria-pressed={mode === "text"} onClick={() => onMode("text")}>
              Text
            </button>
          </div>
          <input
            ref={input}
            value={query}
            aria-label={mode === "files" ? "Find a file by name" : "Search file contents"}
            placeholder={mode === "files" ? "Find a file by name…" : "Search file contents…"}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") choose(0);
            }}
          />
          <button type="button" className="project-files-icon" aria-label="Close search" onClick={onClose}>
            <X aria-hidden="true" size={15} />
          </button>
        </div>
        <div className="project-files-palette-options">
          {mode === "text" && (
            <label>
              <input type="checkbox" checked={matchCase} onChange={(event) => setMatchCase(event.target.checked)} />
              Match case
            </label>
          )}
          <label>
            <input
              type="checkbox"
              checked={includeIgnored}
              onChange={(event) => setIncludeIgnored(event.target.checked)}
            />
            Include ignored files
          </label>
        </div>
        <div className="project-files-palette-list">
          {failure && (
            <div className="project-files-error" role="alert">
              <span>{failure}</span>
              <button type="button" onClick={() => search()}>
                Retry
              </button>
            </div>
          )}
          {entries.map((entry, index) => (
            <button type="button" key={`${entry.path}:${entry.line ?? ""}:${index}`} onClick={() => choose(index)}>
              <File aria-hidden="true" size={14} />
              <span>
                {entry.path}
                {entry.line != null && (
                  <small>
                    {entry.line}: {entry.text}
                  </small>
                )}
              </span>
            </button>
          ))}
          {page.loading && !entries.length && (
            <p className="project-files-note" role="status">
              Searching…
            </p>
          )}
          {!page.loading && !failure && !entries.length && (
            <p className="project-files-note">{query.trim() ? "No matches." : `Search in ${workspaceLabel}.`}</p>
          )}
          {page.result?.truncated && (
            <p className="project-files-note" role="status">
              Results are incomplete because a scan limit was reached. Narrow your search.
            </p>
          )}
          {page.result?.nextCursor != null && (
            <button
              type="button"
              className="project-files-more"
              disabled={page.loading}
              onClick={() => search(page.result?.nextCursor ?? 0)}
            >
              Load more
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
