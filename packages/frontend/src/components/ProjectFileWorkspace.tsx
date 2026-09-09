import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import {
  ArrowLeft,
  ArrowRight,
  ChevronDown,
  ChevronRight,
  File,
  Files,
  Folder,
  GitBranch,
  GitCompareArrows,
  RefreshCw,
  Search,
} from "lucide-react";
import type {
  BackendConnection,
  ProjectFileEntry,
  ProjectFilesParams,
  ProjectFilesResult,
  TaskId,
} from "@openaide/app-server-client";
import { readProjectFiles, type ProjectFileOperation } from "../intents/projectFileIntents";
import { FileViewerPanel } from "./FileViewerPanel";
import { FileCodeReader } from "./FileCodeReader";
import type { useTaskFileViewer } from "./useTaskFileViewer";
import "../styles/app/project-file-workspace.css";

type Page = { result?: ProjectFilesResult; loading: boolean; error?: string };
type Mode = "files" | "search" | "changes";
const reasons: Record<string, string> = {
  unavailable: "Files are unavailable. Try refreshing.",
  permissionDenied: "Permission denied.",
  outsideWorkspace: "This location is outside the Task Workspace.",
  notRepository: "This workspace is not a Git repository root.",
  timeout: "The operation timed out. Try a narrower search or refresh.",
  tooLarge: "This result is too large to display.",
  changed: "This file is no longer changed. Refresh Changes.",
};
function Failure({ page, retry }: { page: Page; retry: () => void }) {
  const message =
    page.error ?? (page.result?.error ? (reasons[page.result.error] ?? "Unable to read files.") : undefined);
  return message ? (
    <div className="project-files-error" role="alert">
      <span>{message}</span>
      <button onClick={retry}>Retry</button>
    </div>
  ) : null;
}
function Limited({ result, more, loading }: { result?: ProjectFilesResult; more: () => void; loading?: boolean }) {
  return (
    <>
      {result?.truncated && (
        <p className="project-files-note" role="status">
          Results are incomplete because a scan or file-size limit was reached. Narrow your search.
        </p>
      )}
      {result?.nextCursor != null && (
        <button className="project-files-more" onClick={more} disabled={loading}>
          Load more
        </button>
      )}
    </>
  );
}
/** Composition of the existing Task-owned viewer. All root-sensitive work goes through
 * task-authorized intents; navigation and selected view are ephemeral presentation. */
export function ProjectFileWorkspace({
  connection,
  taskId,
  workspaceRoot,
  worktreeName,
  gitRef,
  viewer,
  onBack,
  onQuote,
  visible,
  shortcutMode,
  fileRequest,
}: {
  connection: Pick<BackendConnection, "request">;
  taskId: string;
  workspaceRoot: string;
  worktreeName?: string;
  gitRef?: string;
  viewer: ReturnType<typeof useTaskFileViewer>;
  onBack: () => void;
  onQuote?: (text: string) => void;
  visible: boolean;
  fileRequest?: { path: string; line?: number; sequence: number };
  shortcutMode?: { mode: "files" | "search"; sequence: number };
}) {
  const request = connection.request;
  const [mode, setMode] = useState<Mode>("files");
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState("");
  const [matchCase, setMatchCase] = useState(false);
  const [includeIgnored, setIncludeIgnored] = useState(false);
  const [mobile, setMobile] = useState<"list" | "file">("list");
  const [width, setWidth] = useState(254);
  const [expanded, setExpanded] = useState(new Set([""]));
  const [directories, setDirectories] = useState<Record<string, Page>>({});
  const [page, setPage] = useState<Page>({ loading: false });
  const [changesPage, setChangesPage] = useState<Page>({ loading: false });
  const [diffPage, setDiffPage] = useState<Page>({ loading: false });
  const [diffPath, setDiffPath] = useState<string>();
  const [history, setHistory] = useState<{ path: string; line?: number; diff: boolean }[]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [views, setViews] = useState<Record<string, boolean>>({});
  const [refresh, setRefresh] = useState(0);
  const epochs = useRef(new Map<string, number>());
  const search = useRef<HTMLInputElement>(null);
  const active = viewer.activeTab;
  const params = useCallback(
    (extra: Partial<ProjectFilesParams> = {}): ProjectFilesParams => ({
      taskId: taskId as TaskId,
      path: "",
      query: "",
      content: false,
      caseSensitive: false,
      includeIgnored: false,
      cursor: 0,
      ...extra,
    }),
    [taskId],
  );
  const load = useCallback(
    async (
      key: string,
      operation: ProjectFileOperation,
      value: ProjectFilesParams,
      update: (next: Page | ((old: Page) => Page)) => void,
    ) => {
      const epoch = (epochs.current.get(key) ?? 0) + 1;
      epochs.current.set(key, epoch);
      update((old) => ({ ...old, loading: true, error: undefined }));
      try {
        const result = await readProjectFiles({ request }, operation, value);
        if (epochs.current.get(key) !== epoch) return;
        update((old) => ({
          loading: false,
          result:
            value.cursor && old.result ? { ...result, entries: [...old.result.entries, ...result.entries] } : result,
        }));
      } catch (error) {
        if (epochs.current.get(key) === epoch)
          update({ loading: false, error: error instanceof Error ? error.message : "Unable to read project files." });
      }
    },
    [request],
  );
  useEffect(
    () => () => {
      epochs.current.forEach((value, key) => epochs.current.set(key, value + 1));
    },
    [request, taskId, workspaceRoot],
  );
  const directory = useCallback(
    (path: string, cursor = 0) =>
      void load(`directory:${path}`, "files", params({ path, cursor }), (next) =>
        setDirectories((old) => ({
          ...old,
          [path]: typeof next === "function" ? next(old[path] ?? { loading: false }) : next,
        })),
      ),
    [load, params],
  );
  useEffect(() => {
    if (visible) for (const path of expanded) directory(path);
  }, [directory, refresh, visible]);
  const loadPage = useCallback(
    (cursor = 0) =>
      void load(
        "navigator",
        mode === "changes" ? "changes" : "search",
        params({
          query: mode === "search" ? query : filter,
          content: mode === "search",
          caseSensitive: matchCase,
          includeIgnored,
          cursor,
        }),
        setPage,
      ),
    [load, mode, params, query, filter, matchCase, includeIgnored],
  );
  useEffect(() => {
    epochs.current.set("navigator", (epochs.current.get("navigator") ?? 0) + 1);
    setPage({ loading: false });
    if (!visible || (mode !== "changes" && !(mode === "search" ? query : filter).trim())) return;
    const timer = setTimeout(() => loadPage(), mode === "changes" ? 0 : 180);
    return () => {
      clearTimeout(timer);
      epochs.current.set("navigator", (epochs.current.get("navigator") ?? 0) + 1);
    };
  }, [mode, query, filter, visible, refresh, loadPage]);
  useEffect(() => {
    if (visible) void load("changes-summary", "changes", params(), setChangesPage);
  }, [visible, refresh, load, params]);
  const loadDiff = useCallback(() => {
    if (diffPath) void load("diff", "diff", params({ path: diffPath }), setDiffPage);
  }, [diffPath, load, params]);
  useEffect(() => {
    epochs.current.set("diff", (epochs.current.get("diff") ?? 0) + 1);
    setDiffPage({ loading: false });
    loadDiff();
  }, [loadDiff, refresh]);
  useEffect(() => {
    if (active) {
      setMobile("file");
      if (diffPath && active.displayPath !== diffPath && !active.displayPath.endsWith(`/${diffPath}`)) {
        const path = relativePath(active.displayPath);
        setDiffPath(views[path] ? path : undefined);
      }
    }
  }, [active?.handle, active?.displayPath]);
  function relativePath(path: string) {
    const normalized = path.replaceAll("\\", "/");
    const root = workspaceRoot.replaceAll("\\", "/").replace(/\/$/, "");
    return normalized.startsWith(root + "/") ? normalized.slice(root.length + 1) : normalized;
  }
  function visit(path: string, diff: boolean, line?: number, remember = true) {
    setMobile("file");
    setViews((old) => (diff || path in old ? { ...old, [path]: diff } : old));
    setDiffPath(diff ? path : undefined);
    if (
      remember &&
      (history[historyIndex]?.path !== path ||
        history[historyIndex]?.line !== line ||
        history[historyIndex]?.diff !== diff)
    ) {
      setHistory((old) => [...old.slice(0, historyIndex + 1), { path, line, diff }]);
      setHistoryIndex(historyIndex + 1);
    }
    const existing = viewer.tabs.find((tab) => relativePath(tab.displayPath) === path);
    if (existing) {
      viewer.selectTab(existing.handle);
      viewer.focusTab(existing.handle, line);
    } else void viewer.openPath(path, line);
  }
  useEffect(() => {
    // Explicit Chat references join normal navigation and always request source, even
    // when this file's tab previously displayed a diff. The tab owner is unchanged.
    if (fileRequest) visit(relativePath(fileRequest.path), false, fileRequest.line);
  }, [fileRequest]);
  function open(item: ProjectFileEntry, diff = false) {
    visit(item.path, diff, item.line ?? undefined);
  }
  function historyTo(index: number) {
    const item = history[index];
    if (item) {
      setHistoryIndex(index);
      visit(item.path, item.diff, item.line, false);
    }
  }
  function changeMode(next: Mode) {
    setMode(next);
    setMobile("list");
    if (next === "search") requestAnimationFrame(() => search.current?.focus());
  }
  useEffect(() => {
    if (!visible || !shortcutMode) return;
    changeMode(shortcutMode.mode);
    requestAnimationFrame(() =>
      shortcutMode.mode === "search"
        ? search.current?.focus()
        : document.getElementById("project-file-filter")?.focus(),
    );
  }, [shortcutMode]);
  useEffect(() => {
    if (!visible) return;
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) {
        event.preventDefault();
        if (matchMedia("(max-width: 759px)").matches && mobile === "file") setMobile("list");
        else onBack();
      }
      if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLowerCase() === "f") {
        event.preventDefault();
        changeMode("search");
      }
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "p") {
        event.preventDefault();
        changeMode("files");
        requestAnimationFrame(() => document.getElementById("project-file-filter")?.focus());
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [visible, mobile, onBack]);
  const diffLines = useMemo(
    () =>
      diffPage.result?.diff?.hunks.flatMap((hunk) =>
        hunk.lines.map((line, index) => ({ ...line, heading: index === 0 ? hunk.heading : undefined })),
      ) ?? [],
    [diffPage.result],
  );
  function navigateTree(event: React.KeyboardEvent<HTMLButtonElement>, item: ProjectFileEntry) {
    const rows = [
      ...(event.currentTarget
        .closest(".project-files-list")
        ?.querySelectorAll<HTMLButtonElement>(".project-file-row") ?? []),
    ];
    const index = rows.indexOf(event.currentTarget);
    const focus = (next: number) => rows[Math.max(0, Math.min(rows.length - 1, next))]?.focus();
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        focus(index + 1);
        break;
      case "ArrowUp":
        event.preventDefault();
        focus(index - 1);
        break;
      case "Home":
        event.preventDefault();
        focus(0);
        break;
      case "End":
        event.preventDefault();
        focus(rows.length - 1);
        break;
      case "ArrowRight":
        event.preventDefault();
        if (item.directory && !expanded.has(item.path)) event.currentTarget.click();
        else if (item.directory) focus(index + 1);
        break;
      case "ArrowLeft": {
        event.preventDefault();
        if (item.directory && expanded.has(item.path)) event.currentTarget.click();
        else rows.find((row) => row.dataset.path === item.path.split("/").slice(0, -1).join("/"))?.focus();
        break;
      }
    }
  }
  function tree(path: string, depth: number): React.ReactNode {
    const state = directories[path] ?? { loading: true };
    return (
      <>
        <Failure page={state} retry={() => directory(path)} />
        {/* Keep cached rows in place during refresh; inserting a loading row shifts scroll anchoring. */}
        {state.loading && !state.result && (
          <p role="status" className="project-files-note">
            Loading files…
          </p>
        )}
        {state.result?.entries.map((item) => (
          <div key={item.path}>
            <button
              className="project-file-row"
              role="treeitem"
              aria-level={depth + 1}
              data-path={item.path}
              onKeyDown={(event) => navigateTree(event, item)}
              style={{ paddingLeft: 12 + depth * 16 }}
              aria-expanded={item.directory ? expanded.has(item.path) : undefined}
              aria-current={
                !item.directory && (active?.displayPath === item.path || active?.displayPath.endsWith(`/${item.path}`))
                  ? "page"
                  : undefined
              }
              onClick={() => {
                if (!item.directory) {
                  open(item);
                  return;
                }
                setExpanded((old) => {
                  const next = new Set(old);
                  next.has(item.path) ? next.delete(item.path) : next.add(item.path);
                  return next;
                });
                if (!directories[item.path]) directory(item.path);
              }}
            >
              {item.directory ? (
                <>
                  {expanded.has(item.path) ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
                  <Folder size={14} />
                </>
              ) : (
                <>
                  <span className="project-file-indent" />
                  <File size={14} />
                </>
              )}
              {item.name}
            </button>
            {item.directory && expanded.has(item.path) && tree(item.path, depth + 1)}
          </div>
        ))}
        <Limited
          loading={state.loading}
          result={state.result}
          more={() => directory(path, state.result?.nextCursor ?? 0)}
        />
      </>
    );
  }
  const changed = changesPage.result?.entries.find(
    (item) => item.path === active?.displayPath || active?.displayPath.endsWith(`/${item.path}`),
  );
  return (
    <section className="project-file-workspace" hidden={!visible} aria-label="Project files">
      <header className="project-files-header">
        <button onClick={onBack}>
          <ArrowLeft size={15} />
          Back to conversation
        </button>
        <strong title={workspaceRoot}>
          {worktreeName ?? workspaceRoot.split(/[/\\]/).filter(Boolean).at(-1) ?? "Project"}
        </strong>
        <span>
          <GitBranch size={13} />
          {changesPage.result?.branch ?? gitRef ?? "Project root"}
        </span>
        <button
          className="project-files-refresh"
          aria-label="Refresh project files"
          onClick={() => {
            setDirectories({});
            setRefresh((value) => value + 1);
          }}
        >
          <RefreshCw size={14} />
        </button>
      </header>
      <div
        className="project-files-body"
        data-mobile-page={mobile}
        style={{ "--project-navigation-width": `${width}px` } as CSSProperties}
      >
        <aside className="project-files-navigation" aria-label="Project navigation">
          <nav aria-label="Browse project">
            {(
              [
                ["files", Files, "Files"],
                ["search", Search, "Search"],
                ["changes", GitCompareArrows, "Changes"],
              ] as const
            ).map(([key, Icon, label]) => (
              <button key={key} aria-pressed={mode === key} onClick={() => changeMode(key)}>
                <Icon size={14} />
                {label}
              </button>
            ))}
          </nav>
          {mode !== "changes" && (
            <label className="project-files-input">
              <Search size={14} />
              <input
                ref={mode === "search" ? search : undefined}
                id={mode === "files" ? "project-file-filter" : undefined}
                aria-label={mode === "files" ? "Find a file" : "Search file contents"}
                placeholder={mode === "files" ? "Find a file…" : "Search file contents…"}
                value={mode === "files" ? filter : query}
                onChange={(event) => (mode === "files" ? setFilter(event.target.value) : setQuery(event.target.value))}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && page.result?.entries[0]) open(page.result.entries[0]);
                }}
              />
              {mode === "search" && (
                <button aria-label="Match case" aria-pressed={matchCase} onClick={() => setMatchCase(!matchCase)}>
                  Aa
                </button>
              )}
            </label>
          )}
          {mode !== "changes" && (mode === "search" || filter) && (
            <label className="project-files-options">
              <input
                type="checkbox"
                checked={includeIgnored}
                onChange={(event) => setIncludeIgnored(event.target.checked)}
              />
              Include ignored files
            </label>
          )}
          <div
            className="project-files-list"
            role={mode === "files" && !filter ? "tree" : undefined}
            aria-label={mode === "files" && !filter ? "Workspace files" : undefined}
          >
            {mode === "files" && !filter ? (
              tree("", 0)
            ) : (
              <>
                {page.loading && (
                  <p className="project-files-note" role="status">
                    {mode === "changes" ? "Reading changes…" : "Searching…"}
                  </p>
                )}
                <Failure page={page} retry={() => loadPage()} />
                {page.result?.entries.map((item, index) => (
                  <button
                    className="project-file-result"
                    key={`${item.path}:${item.line ?? index}`}
                    onClick={() => open(item, mode === "changes")}
                  >
                    <File size={14} />
                    <span>
                      {item.path}
                      <small>{item.line ? `Line ${item.line} · ${item.text}` : item.status}</small>
                    </span>
                  </button>
                ))}
                {!page.loading && !page.error && !page.result?.error && !page.result?.entries.length && (
                  <p className="project-files-note">
                    {mode === "changes"
                      ? "No working changes."
                      : (mode === "search" ? query : filter)
                        ? "No matches."
                        : "Search across file contents."}
                  </p>
                )}
                <Limited
                  loading={page.loading}
                  result={page.result}
                  more={() => loadPage(page.result?.nextCursor ?? 0)}
                />
              </>
            )}
          </div>
        </aside>
        <div
          className="project-files-resizer"
          role="separator"
          aria-label="Resize project navigation"
          aria-orientation="vertical"
          aria-valuemin={210}
          aria-valuemax={400}
          aria-valuenow={width}
          tabIndex={0}
          onKeyDown={(event) => {
            if (["ArrowLeft", "ArrowRight"].includes(event.key)) {
              event.preventDefault();
              setWidth((old) => Math.max(210, Math.min(400, old + (event.key === "ArrowRight" ? 20 : -20))));
            }
          }}
          onPointerDown={(event) => event.currentTarget.setPointerCapture(event.pointerId)}
          onPointerMove={(event) => {
            if (event.currentTarget.hasPointerCapture(event.pointerId))
              setWidth((old) => Math.max(210, Math.min(400, old + event.movementX)));
          }}
          onPointerUp={(event) => event.currentTarget.releasePointerCapture(event.pointerId)}
        />
        <div className="project-files-reader">
          <div className="project-files-toolbar">
            <button
              className="project-files-history"
              aria-label="Back"
              disabled={historyIndex <= 0}
              onClick={() => historyTo(historyIndex - 1)}
            >
              <ArrowLeft size={14} />
            </button>
            <button
              className="project-files-history"
              aria-label="Forward"
              disabled={historyIndex >= history.length - 1}
              onClick={() => historyTo(historyIndex + 1)}
            >
              <ArrowRight size={14} />
            </button>
            <button className="project-files-mobile-back" onClick={() => setMobile("list")}>
              <ArrowLeft size={14} />
              {mode === "search" ? "Results" : mode === "changes" ? "Changes" : "Files"}
            </button>
            <span title={active?.displayPath}>{active ? relativePath(active.displayPath) : "Project files"}</span>
            {(diffPath || changed || (active && relativePath(active.displayPath) in views)) && (
              <div>
                <button
                  aria-pressed={!diffPath}
                  onClick={() => active && visit(relativePath(active.displayPath), false)}
                >
                  File
                </button>
                <button
                  aria-pressed={Boolean(diffPath)}
                  onClick={() => active && visit(relativePath(active.displayPath), true)}
                >
                  Diff
                </button>
              </div>
            )}
          </div>
          <FileViewerPanel
            collapsed={false}
            tab={active}
            tabs={viewer.tabs}
            onClose={viewer.closeTab}
            onSelect={(handle) => {
              const tab = viewer.tabs.find((item) => item.handle === handle);
              if (tab) {
                const path = relativePath(tab.displayPath);
                visit(path, views[path] ?? false);
              }
            }}
            onRefresh={(handle) => {
              void viewer.refresh(handle);
              if (diffPath) loadDiff();
            }}
            onOpenFromHandle={viewer.openFromHandle}
            onQuote={
              onQuote
                ? (text) => {
                    onBack();
                    onQuote(text);
                  }
                : undefined
            }
            splitRatio={0.5}
            onSplitRatio={() => undefined}
            content={
              diffPath ? (
                <div className="project-files-diff">
                  <Failure page={diffPage} retry={loadDiff} />
                  {diffPage.loading ? (
                    <p role="status">Loading diff…</p>
                  ) : diffPage.result?.diff?.conflicted ? (
                    <p>Conflicted file. Open File to inspect the working copy; resolve it in your editor.</p>
                  ) : diffPage.result?.diff?.binary ? (
                    <p>Binary file changed. Open File for its preview.</p>
                  ) : diffPage.result?.diff && !diffLines.length ? (
                    <p>No textual changes compared with HEAD.</p>
                  ) : (
                    <FileCodeReader
                      key={diffPath}
                      lines={diffLines}
                      language={diffPath.split(".").at(-1)}
                      path={diffPath}
                      diff
                      onQuote={
                        onQuote
                          ? (text) => {
                              onBack();
                              onQuote(text);
                            }
                          : undefined
                      }
                    />
                  )}
                </div>
              ) : undefined
            }
          />
          {!viewer.tabs.length && (
            <div className="project-files-empty">
              <Files size={28} />
              <strong>Explore your project</strong>
              <p>Choose a file, search its contents, or inspect working changes.</p>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
