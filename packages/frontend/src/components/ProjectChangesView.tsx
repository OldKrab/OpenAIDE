import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ChevronDown, ChevronLeft, ChevronRight, File, Folder, FolderOpen, GitCompareArrows } from "lucide-react";
import type { BackendConnection, ProjectChangeBase, ProjectFileEntry } from "@openaide/app-server-client";
import { FileCodeReader } from "./FileCodeReader";
import type { FileReaderNotes } from "./FileViewerPanel";
import {
  buildChangeTree,
  changeStatusLetter,
  scopeKey,
  scopeParams,
  type ChangeTreeNode,
  type FileChangesLayout,
  type FileChangesMode,
  type FileScope,
} from "./projectFilesModel";
import { pageError, useProjectFileLoader, type FilePage } from "./useProjectFiles";

type Connection = Pick<BackendConnection, "request">;
/** Notes and comment actions for one changed file; absent actions leave the diff read-only. */
export type DiffNotes = FileReaderNotes;

export function ChangeCounts({ entry }: { entry: Pick<ProjectFileEntry, "added" | "removed" | "status"> }) {
  const letter = changeStatusLetter(entry.status);
  return (
    <span className="project-files-counts">
      {(entry.added ?? 0) > 0 && <span className="project-files-added">+{entry.added}</span>}
      {(entry.removed ?? 0) > 0 && <span className="project-files-removed">−{entry.removed}</span>}
      <span className="project-files-status" data-status={letter} title={entry.status ?? "modified"}>
        {letter}
      </span>
    </span>
  );
}

export function ChangeSummary({ count, added, removed }: { count: number; added: number; removed: number }) {
  return (
    <span>
      {count} {count === 1 ? "file" : "files"} changed <span className="project-files-added">+{added}</span>{" "}
      <span className="project-files-removed">−{removed}</span>
    </span>
  );
}

const directoryOf = (path: string) => (path.includes("/") ? `${path.split("/").slice(0, -1).join("/")}/` : "");
const nameOf = (path: string) => path.split("/").at(-1) ?? path;

/** Changed files only, as a folder tree or a flat list. Folder state is the owner's so it
 * survives a refresh and a switch to Files and back. */
export function ProjectChangesList({
  entries,
  layout,
  selected,
  closed,
  onToggleFolder,
  onPick,
}: {
  entries: ProjectFileEntry[];
  layout: FileChangesLayout;
  selected?: string;
  closed: Set<string>;
  onToggleFolder: (path: string) => void;
  onPick: (path: string) => void;
}) {
  const byPath = useMemo(() => new Map(entries.map((entry) => [entry.path, entry])), [entries]);
  const tree = useMemo(() => buildChangeTree(entries.map((entry) => entry.path)), [entries]);
  function row(path: string, label: ReactNode, depth: number) {
    const entry = byPath.get(path);
    return (
      <button
        type="button"
        key={path}
        className="project-files-row"
        data-deleted={entry?.status === "deleted"}
        aria-current={selected === path ? "true" : undefined}
        title={entry?.oldPath ? `${entry.oldPath} → ${path}` : path}
        style={{ paddingLeft: 10 + depth * 14 }}
        onClick={() => onPick(path)}
      >
        <span className="project-files-chevron" />
        <File aria-hidden="true" size={14} />
        <span className="project-files-row-label">{label}</span>
        {entry && <ChangeCounts entry={entry} />}
      </button>
    );
  }
  function rows(nodes: ChangeTreeNode[], depth: number): ReactNode {
    return nodes.map((node) =>
      node.children ? (
        <div key={node.path}>
          <button
            type="button"
            className="project-files-row"
            aria-expanded={!closed.has(node.path)}
            style={{ paddingLeft: 10 + depth * 14 }}
            onClick={() => onToggleFolder(node.path)}
          >
            {closed.has(node.path) ? (
              <ChevronRight aria-hidden="true" size={13} className="project-files-chevron" />
            ) : (
              <ChevronDown aria-hidden="true" size={13} className="project-files-chevron" />
            )}
            {closed.has(node.path) ? <Folder aria-hidden="true" size={14} /> : <FolderOpen aria-hidden="true" size={14} />}
            <span className="project-files-row-label">{node.label}</span>
          </button>
          {!closed.has(node.path) && rows(node.children, depth + 1)}
        </div>
      ) : (
        row(node.path, node.label, depth)
      ),
    );
  }
  return layout === "tree"
    ? rows(tree, 0)
    : [...entries]
        .sort((a, b) => a.path.localeCompare(b.path))
        .map((entry) =>
          row(
            entry.path,
            <>
              {nameOf(entry.path)}
              <small>{directoryOf(entry.path).replace(/\/$/, "")}</small>
            </>,
            0,
          ),
        );
}

/** One file's diff. A section far from the viewport waits until it is scrolled near, so a
 * large change set does not read every diff at once. */
function ChangeDiff({
  connection,
  scope,
  base,
  baseLabel,
  path,
  refresh,
  lazy,
  reader,
}: {
  connection: Connection;
  scope: FileScope;
  base: ProjectChangeBase;
  baseLabel: string;
  path: string;
  refresh: number;
  lazy: boolean;
  reader: DiffNotes;
}) {
  const identity = `${scopeKey(scope)}|${base}`;
  const { load } = useProjectFileLoader(connection, identity);
  const [page, setPage] = useState<FilePage>({ loading: false });
  const [near, setNear] = useState(!lazy || typeof IntersectionObserver === "undefined");
  const holder = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (near || !holder.current) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) setNear(true);
      },
      { rootMargin: "600px" },
    );
    observer.observe(holder.current);
    return () => observer.disconnect();
  }, [near]);
  const read = () => void load("diff", "diff", scopeParams(scope, { path, base }), setPage);
  useEffect(() => {
    if (near) read();
  }, [near, load, identity, path, refresh]);
  const lines = useMemo(
    () =>
      page.result?.diff?.hunks.flatMap((hunk) =>
        hunk.lines.map((line, index) => ({ ...line, heading: index === 0 ? hunk.heading : undefined })),
      ) ?? [],
    [page.result],
  );
  const failure = pageError(page);
  const diff = page.result?.diff;
  return (
    <div ref={holder} className="project-files-diff" data-pending={!page.result && !failure}>
      {failure ? (
        <div className="project-files-error" role="alert">
          <span>{failure}</span>
          <button type="button" onClick={read}>
            Retry
          </button>
        </div>
      ) : !page.result ? (
        <p role="status">{near ? "Loading diff…" : ""}</p>
      ) : diff?.conflicted ? (
        <p>Conflicted file. Open it in Files to inspect the working copy; resolve it in your editor.</p>
      ) : diff?.binary ? (
        <p>Binary file changed. Open it in Files for a preview.</p>
      ) : !lines.length ? (
        <p>No textual changes compared with {baseLabel}.</p>
      ) : (
        <FileCodeReader
          lines={lines}
          language={path.split(".").at(-1)}
          path={reader.path}
          diff
          baseLabel={baseLabel}
          notes={reader.notes}
          onSaveNote={reader.onSaveNote}
          onRemoveNote={reader.onRemoveNote}
          submitLabel={reader.submitLabel}
        />
      )}
    </div>
  );
}

/** The diffs of the change list: every file on one page, or one file at a time. */
export function ProjectChangesReader({
  connection,
  scope,
  base,
  baseLabel,
  entries,
  mode,
  shown,
  onShow,
  folded,
  onFold,
  refresh,
  reader,
  onOpenFile,
  sections,
  empty,
}: {
  connection: Connection;
  scope: FileScope;
  base: ProjectChangeBase;
  /** Name of the compared side in messages and quote references. */
  baseLabel: string;
  entries: ProjectFileEntry[];
  mode: FileChangesMode;
  /** The file shown in one-file mode. */
  shown?: string;
  onShow: (path: string) => void;
  folded: Set<string>;
  onFold: (path: string) => void;
  refresh: number;
  reader: (path: string) => DiffNotes;
  onOpenFile: (path: string) => void;
  /** Section elements by path, so the list can scroll one into view. */
  sections: Map<string, HTMLElement>;
  empty: ReactNode;
}) {
  if (!entries.length)
    return (
      <div className="project-files-empty">
        <GitCompareArrows aria-hidden="true" size={26} />
        {empty}
      </div>
    );
  const diff = (entry: ProjectFileEntry, lazy: boolean) => (
    <ChangeDiff
      key={`${entry.path}:${base}`}
      connection={connection}
      scope={scope}
      base={base}
      baseLabel={baseLabel}
      path={entry.path}
      refresh={refresh}
      lazy={lazy}
      reader={reader(entry.path)}
    />
  );
  if (mode === "all")
    return (
      <div className="project-files-stack">
        {entries.map((entry) => (
          <section
            key={entry.path}
            aria-label={entry.path}
            ref={(element) => void (element ? sections.set(entry.path, element) : sections.delete(entry.path))}
          >
            <header className="project-files-file-head">
              <button
                type="button"
                className="project-files-file-fold"
                aria-expanded={!folded.has(entry.path)}
                onClick={() => onFold(entry.path)}
              >
                {folded.has(entry.path) ? <ChevronRight aria-hidden="true" size={14} /> : <ChevronDown aria-hidden="true" size={14} />}
                <span className="project-files-file-dir">{directoryOf(entry.path)}</span>
                <strong>{nameOf(entry.path)}</strong>
              </button>
              <ChangeCounts entry={entry} />
              {entry.status !== "deleted" && (
                <button type="button" className="project-files-link" onClick={() => onOpenFile(entry.path)}>
                  Open in Files
                </button>
              )}
            </header>
            {!folded.has(entry.path) && diff(entry, true)}
          </section>
        ))}
      </div>
    );
  const index = Math.max(
    0,
    entries.findIndex((entry) => entry.path === shown),
  );
  const entry = entries[index];
  return (
    <div className="project-files-one">
      <header className="project-files-file-head">
        <span className="project-files-file-dir">{directoryOf(entry.path)}</span>
        <strong>{nameOf(entry.path)}</strong>
        <ChangeCounts entry={entry} />
        {entry.status !== "deleted" && (
          <button type="button" className="project-files-link" onClick={() => onOpenFile(entry.path)}>
            Open in Files
          </button>
        )}
        <span className="project-files-step">
          <button
            type="button"
            aria-label="Previous changed file"
            disabled={index <= 0}
            onClick={() => onShow(entries[index - 1].path)}
          >
            <ChevronLeft aria-hidden="true" size={16} />
          </button>
          <small>
            {index + 1} of {entries.length}
          </small>
          <button
            type="button"
            aria-label="Next changed file"
            disabled={index >= entries.length - 1}
            onClick={() => onShow(entries[index + 1].path)}
          >
            <ChevronRight aria-hidden="true" size={16} />
          </button>
        </span>
      </header>
      {diff(entry, false)}
    </div>
  );
}
