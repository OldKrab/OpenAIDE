import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  ArrowLeft,
  File,
  FolderGit2,
  GitCompareArrows,
  List,
  ListTree,
  Maximize2,
  Minimize2,
  RefreshCw,
  Rows3,
  Search,
  X,
} from "lucide-react";
import type { BackendConnection } from "@openaide/app-server-client";
import { FileViewerPanel, type FileReaderNotes } from "./FileViewerPanel";
import {
  ChangeSummary,
  ProjectChangesList,
  ProjectChangesReader,
} from "./ProjectChangesView";
import {
  ProjectFilesPalette,
  type FileSearchMode,
} from "./ProjectFilesPalette";
import { ProjectFilesTree } from "./ProjectFilesTree";
import { ProjectFilesWorkspaceSwitcher } from "./ProjectFilesWorkspaceSwitcher";
import { TaskPanelSplitter } from "./TaskPanelSplitter";
import {
  changeTotals,
  readFilePreferences,
  writeFilePreferences,
  type FileChangesLayout,
  type FileChangesMode,
  type FileNote,
  type FileWorkspace,
} from "./projectFilesModel";
import { pageError, type useProjectChanges } from "./useProjectFiles";
import type { useTaskFileViewer } from "./useTaskFileViewer";
import { useBackNavigation } from "./useBackNavigation";
import "../styles/app/project-files-panel.css";

type View = "changes" | "files";
export type ProjectFileRequest = {
  path: string;
  line?: number;
  sequence: number;
};
export type ProjectFileSearchRequest = {
  mode: FileSearchMode;
  sequence: number;
};

/** The phone layout starts at the same width as the rest of the Task Page. */
export function useNarrowFilesLayout() {
  const query = "(max-width: 760px)";
  const [narrow, setNarrow] = useState(
    () => typeof matchMedia === "function" && matchMedia(query).matches,
  );
  useEffect(() => {
    if (typeof matchMedia !== "function") return;
    const media = matchMedia(query);
    const update = () => setNarrow(media.matches);
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  return narrow;
}

/**
 * Project files for one workspace: Changes and Files as separate views with their own state.
 * Beside Chat it is a Task Panel with a divider; from a Project it fills the page. Everything
 * here is ephemeral presentation; App Server owns the workspace, the base and every result.
 */
export function ProjectFilesPanel({
  agentName,
  changes,
  collapsed = false,
  commentCount = 0,
  connection,
  fileRequest,
  maximized = false,
  notes,
  onClose,
  onDismissSuggestion,
  onMaximized,
  onRefresh,
  onRemoveNote,
  onSaveNote,
  onSplitRatio,
  onWorkspace,
  refresh,
  searchRequest,
  splitRatio,
  submitLabel,
  headerControl,
  suggestion,
  variant,
  viewer,
  workspace,
  workspaces,
}: {
  agentName?: string;
  changes: ReturnType<typeof useProjectChanges>;
  /** A closed Task Panel stays mounted to keep its view state, out of reach of focus. */
  collapsed?: boolean;
  /** Notes waiting in the message; shown on the phone's way back to Chat. */
  commentCount?: number;
  connection: Pick<BackendConnection, "request">;
  /** A file to open in Files, such as a path clicked in Chat. */
  fileRequest?: ProjectFileRequest;
  maximized?: boolean;
  notes: FileNote[];
  onClose: () => void;
  onDismissSuggestion?: () => void;
  onMaximized?: (maximized: boolean) => void;
  onRefresh: () => void;
  onRemoveNote?: (note: FileNote) => void;
  /** Saves a note; `replaces` is the note being edited. Absent makes every reader read-only. */
  onSaveNote?: (note: FileNote, replaces?: FileNote) => void;
  onSplitRatio?: (ratio: number) => void;
  onWorkspace: (id: string) => void;
  /** Bumped by the owner on open, on turn end and on request. */
  refresh: number;
  searchRequest?: ProjectFileSearchRequest;
  splitRatio?: number;
  submitLabel?: string;
  /** A control the owner places in the header before search, such as the Project page's Ask. */
  headerControl?: ReactNode;
  /** Another workspace the Agent is working in. Offered, never selected for the user. */
  suggestion?: FileWorkspace;
  /** `task` sits beside Chat and sends comments to its Composer; `project` is a full page. */
  variant: "task" | "project";
  viewer: ReturnType<typeof useTaskFileViewer>;
  workspace: FileWorkspace;
  workspaces: FileWorkspace[];
}) {
  const narrow = useNarrowFilesLayout();
  const [view, setView] = useState<View>(
    variant === "task" ? "changes" : "files",
  );
  const [phonePage, setPhonePage] = useState<"list" | "reader">("list");
  const [preferences, setPreferences] = useState(readFilePreferences);
  const [closedFolders, setClosedFolders] = useState(new Set<string>());
  const [folded, setFolded] = useState(new Set<string>());
  const [expanded, setExpanded] = useState(new Set<string>());
  const [shownDiff, setShownDiff] = useState<string>();
  const [palette, setPalette] = useState<FileSearchMode>();
  const sections = useRef(new Map<string, HTMLElement>());
  const shown: View = changes.repository ? view : "files";
  const [treeSeen, setTreeSeen] = useState(false);
  if (shown === "files" && !treeSeen) setTreeSeen(true);
  const [changesSeen, setChangesSeen] = useState(false);
  if (shown === "changes" && !changesSeen) setChangesSeen(true);
  const entries = changes.entries;
  const changed = useMemo(
    () => new Map(entries.map((entry) => [entry.path, entry])),
    [entries],
  );
  const totals = useMemo(() => changeTotals(entries), [entries]);
  const active = viewer.activeTab;
  const baseLabel =
    changes.base === "defaultBranch"
      ? (changes.defaultBranch ?? "the default branch")
      : "HEAD";
  const home =
    variant === "task" ? workspace.taskWorkspace : workspace.id === "root";

  useBackNavigation(
    narrow && !collapsed && phonePage === "reader",
    () => setPhonePage("list"),
    110,
  );
  // Per-workspace view state must not leak into the workspace selected next.
  useEffect(() => {
    setClosedFolders(new Set());
    setFolded(new Set());
    setExpanded(new Set());
    setShownDiff(undefined);
    setPhonePage("list");
  }, [workspace.id]);

  function relative(path: string) {
    const normalized = path.replaceAll("\\", "/");
    const root = workspace.path.replaceAll("\\", "/").replace(/\/$/, "");
    return normalized.startsWith(`${root}/`)
      ? normalized.slice(root.length + 1)
      : normalized;
  }
  /** A comment made outside the home workspace names that folder, so the Agent cannot mistake the file. */
  const quotePath = (path: string) =>
    home ? path : `${workspace.path.replace(/[/\\]$/, "")}/${path}`;
  function prefer(update: {
    layout?: FileChangesLayout;
    mode?: FileChangesMode;
  }) {
    setPreferences(writeFilePreferences((old) => ({ ...old, ...update })));
  }
  function toggle(
    set: (update: (old: Set<string>) => Set<string>) => void,
    key: string,
  ) {
    set((old) => {
      const next = new Set(old);
      next.has(key) ? next.delete(key) : next.add(key);
      return next;
    });
  }
  function openFile(path: string, line?: number) {
    setView("files");
    setPhonePage("reader");
    const existing = viewer.tabs.find(
      (tab) => relative(tab.displayPath) === path,
    );
    if (existing) {
      viewer.selectTab(existing.handle);
      viewer.focusTab(existing.handle, line);
    } else void viewer.openPath(path, line);
  }
  function openChange(path: string) {
    setView("changes");
    setPhonePage("reader");
    setShownDiff(path);
    if (preferences.mode === "file") return;
    setFolded((old) => {
      const next = new Set(old);
      next.delete(path);
      return next;
    });
    requestAnimationFrame(() =>
      sections.current.get(path)?.scrollIntoView?.({ block: "start" }),
    );
  }
  useEffect(() => {
    if (fileRequest) openFile(relative(fileRequest.path), fileRequest.line);
  }, [fileRequest?.sequence]);
  useEffect(() => {
    if (searchRequest) setPalette(searchRequest.mode);
  }, [searchRequest?.sequence]);

  function reader(kind: string, path: string): FileReaderNotes {
    const key = `${workspace.id}:${kind}:${path}`;
    return {
      path: quotePath(path),
      notes: notes.filter((note) => note.key === key),
      onSaveNote: onSaveNote
        ? (draft, replaces) =>
            onSaveNote(
              { ...draft, id: replaces ?? crypto.randomUUID(), key },
              notes.find((note) => note.id === replaces),
            )
        : undefined,
      onRemoveNote: onRemoveNote
        ? (id) => {
            const note = notes.find((item) => item.id === id);
            if (note) onRemoveNote(note);
          }
        : undefined,
      submitLabel,
    };
  }

  const activePath = active ? relative(active.displayPath) : undefined;
  const failure = pageError(changes.page);
  const modeSwitch = (
    <div
      className="project-files-segment project-files-mode"
      role="group"
      aria-label="Show changes as"
    >
      <button
        type="button"
        aria-pressed={preferences.mode === "all"}
        aria-label="All changes on one page"
        title="All changes on one page"
        onClick={() => prefer({ mode: "all" })}
      >
        <Rows3 aria-hidden="true" size={14} />
      </button>
      <button
        type="button"
        aria-pressed={preferences.mode === "file"}
        aria-label="One file at a time"
        title="One file at a time"
        onClick={() => prefer({ mode: "file" })}
      >
        <File aria-hidden="true" size={14} />
      </button>
    </div>
  );
  const back = narrow && (
    <button
      type="button"
      className="project-files-back"
      onClick={() =>
        phonePage === "reader" ? setPhonePage("list") : onClose()
      }
    >
      <ArrowLeft aria-hidden="true" size={16} />
      {phonePage === "reader"
        ? shown === "changes"
          ? "Changes"
          : "Files"
        : variant === "project"
          ? "Back"
          : commentCount
            ? `Chat · ${commentCount} ${commentCount === 1 ? "comment" : "comments"}`
            : "Chat"}
    </button>
  );
  const body = (
    <>
      <header className="project-files-head">
        {back}
        {changes.repository && (
          <div
            className="project-files-segment project-files-view"
            role="group"
            aria-label="View"
          >
            <button
              type="button"
              aria-pressed={shown === "changes"}
              onClick={() => setView("changes")}
            >
              Changes <small>{changes.total}</small>
            </button>
            <button
              type="button"
              aria-pressed={shown === "files"}
              onClick={() => setView("files")}
            >
              Files
            </button>
          </div>
        )}
        <ProjectFilesWorkspaceSwitcher
          homeLabel={variant === "task" ? "this Task" : undefined}
          onSelect={onWorkspace}
          suggestedId={suggestion?.id}
          workspace={workspace}
          workspaces={workspaces}
        />
        <span className="project-files-spacer" />
        {headerControl}
        {narrow && phonePage === "reader" && shown === "changes" && modeSwitch}
        <button
          type="button"
          className="project-files-icon"
          aria-label="Search files"
          title="Find a file · Ctrl P"
          onClick={() => setPalette("files")}
        >
          <Search aria-hidden="true" size={15} />
        </button>
        <button
          type="button"
          className="project-files-icon"
          aria-label="Refresh project files"
          title="Refresh"
          aria-busy={changes.page.loading}
          onClick={() => {
            onRefresh();
            if (active && active.kind !== "pending")
              void viewer.refresh(active.handle);
          }}
        >
          <RefreshCw aria-hidden="true" size={14} />
        </button>
        {!narrow && variant === "task" && onMaximized && (
          <button
            type="button"
            className="project-files-icon"
            aria-label={maximized ? "Show Chat" : "Maximize"}
            title={maximized ? "Show Chat" : "Maximize"}
            onClick={() => onMaximized(!maximized)}
          >
            {maximized ? (
              <Minimize2 aria-hidden="true" size={14} />
            ) : (
              <Maximize2 aria-hidden="true" size={14} />
            )}
          </button>
        )}
        {!narrow && (
          <button
            type="button"
            className="project-files-icon"
            aria-label="Close Project files"
            title="Close"
            onClick={onClose}
          >
            <X aria-hidden="true" size={15} />
          </button>
        )}
      </header>
      {suggestion && (
        <div className="project-files-hint" role="status">
          <FolderGit2 aria-hidden="true" size={14} />
          <span>
            {agentName ?? "The Agent"} is working in{" "}
            <strong>{suggestion.label}</strong>
          </span>
          <button
            type="button"
            className="project-files-link"
            onClick={() => onWorkspace(suggestion.id)}
          >
            Switch
          </button>
          <button
            type="button"
            className="project-files-icon"
            aria-label="Dismiss"
            onClick={onDismissSuggestion}
          >
            <X aria-hidden="true" size={13} />
          </button>
        </div>
      )}
      <div className="project-files-body">
        <aside
          className="project-files-nav"
          aria-label={shown === "changes" ? "Changed files" : "Files"}
        >
          {shown === "changes" && (
            <div className="project-files-nav-head">
              {changes.defaultBranch ? (
                <div
                  className="project-files-segment"
                  role="group"
                  aria-label="Compare with"
                >
                  <button
                    type="button"
                    aria-pressed={changes.base === "uncommitted"}
                    onClick={() => changes.setBase("uncommitted")}
                  >
                    Uncommitted
                  </button>
                  <button
                    type="button"
                    aria-pressed={changes.base === "defaultBranch"}
                    onClick={() => changes.setBase("defaultBranch")}
                  >
                    Since {changes.defaultBranch}
                  </button>
                </div>
              ) : (
                <span className="project-files-nav-title">Uncommitted</span>
              )}
              <button
                type="button"
                className="project-files-icon"
                aria-label={
                  preferences.layout === "tree"
                    ? "Show as list"
                    : "Show as tree"
                }
                title={
                  preferences.layout === "tree"
                    ? "Show as list"
                    : "Show as tree"
                }
                onClick={() =>
                  prefer({
                    layout: preferences.layout === "tree" ? "flat" : "tree",
                  })
                }
              >
                {preferences.layout === "tree" ? (
                  <List aria-hidden="true" size={15} />
                ) : (
                  <ListTree aria-hidden="true" size={15} />
                )}
              </button>
            </div>
          )}
          {narrow && shown === "changes" && entries.length > 0 && (
            <p className="project-files-summary">
              <ChangeSummary count={changes.total} {...totals} />
            </p>
          )}
          <div
            className="project-files-list"
            data-view="changes"
            hidden={shown !== "changes"}
          >
            {failure && (
              <div className="project-files-error" role="alert">
                <span>{failure}</span>
                <button type="button" onClick={onRefresh}>
                  Retry
                </button>
              </div>
            )}
            {changes.page.loading && !changes.page.result && (
              <p className="project-files-note" role="status">
                Reading changes…
              </p>
            )}
            {!failure && changes.page.result && !entries.length && (
              <p className="project-files-note">Nothing has changed here.</p>
            )}
            <ProjectChangesList
              closed={closedFolders}
              entries={entries}
              layout={preferences.layout}
              onPick={openChange}
              onToggleFolder={(path) => toggle(setClosedFolders, path)}
              selected={
                preferences.mode === "file"
                  ? (shownDiff ?? entries[0]?.path)
                  : undefined
              }
            />
            {changes.page.result?.truncated && (
              <p className="project-files-note" role="status">
                The change list is incomplete because a limit was reached.
              </p>
            )}
            {changes.page.result?.nextCursor != null && (
              <button
                type="button"
                className="project-files-more"
                disabled={changes.page.loading}
                onClick={changes.loadMore}
              >
                Load more
              </button>
            )}
          </div>
          {/* Once visited, the tree stays mounted behind Changes so its folders and scroll position survive. */}
          {(shown === "files" || treeSeen) && (
            <div
              className="project-files-list"
              data-view="files"
              hidden={shown !== "files"}
            >
              <ProjectFilesTree
                key={workspace.id}
                changes={changed}
                connection={connection}
                expanded={expanded}
                onExpanded={setExpanded}
                onOpen={openFile}
                refresh={refresh}
                scope={workspace.scope}
                selected={activePath}
              />
            </div>
          )}
        </aside>
        <div className="project-files-reader">
          {/* Diffs already read stay mounted behind Files, so coming back neither reloads nor rescrolls. */}
          {(shown === "changes" || changesSeen) && (
            <div
              className="project-files-reader-view"
              hidden={shown !== "changes"}
            >
              {!narrow && entries.length > 0 && (
                <div className="project-files-viewbar">
                  <ChangeSummary count={changes.total} {...totals} />
                  {modeSwitch}
                </div>
              )}
              <div className="project-files-reader-scroll">
                <ProjectChangesReader
                  base={changes.requestedBase}
                  baseLabel={baseLabel}
                  connection={connection}
                  empty={
                    <>
                      <strong>
                        {changes.page.loading && !changes.page.result
                          ? "Reading changes…"
                          : `No ${changes.base === "uncommitted" ? "uncommitted changes" : `changes since ${baseLabel}`} in ${workspace.label}`}
                      </strong>
                      {changes.base === "uncommitted" &&
                        changes.defaultBranch && (
                          <button
                            type="button"
                            className="project-files-link"
                            onClick={() => changes.setBase("defaultBranch")}
                          >
                            Show changes since {changes.defaultBranch}
                          </button>
                        )}
                    </>
                  }
                  entries={entries}
                  folded={folded}
                  mode={preferences.mode}
                  onFold={(path) => toggle(setFolded, path)}
                  onOpenFile={openFile}
                  onShow={setShownDiff}
                  reader={(path) => reader(changes.base, path)}
                  refresh={refresh}
                  scope={workspace.scope}
                  sections={sections.current}
                  shown={shownDiff}
                />
              </div>
            </div>
          )}
          {shown !== "files" ? null : viewer.tabs.length ? (
            <FileViewerPanel
              headerAction={
                activePath && changed.has(activePath) ? (
                  <button
                    type="button"
                    className="project-files-link"
                    onClick={() => openChange(activePath)}
                  >
                    Show diff
                  </button>
                ) : undefined
              }
              onClose={viewer.closeTab}
              onOpenFromHandle={viewer.openFromHandle}
              onRefresh={(handle) => void viewer.refresh(handle)}
              onSelect={viewer.selectTab}
              readerNotes={(displayPath) =>
                reader("file", relative(displayPath))
              }
              tab={active}
              tabs={viewer.tabs}
            />
          ) : (
            <div className="project-files-empty">
              <GitCompareArrows aria-hidden="true" size={26} />
              <strong>Choose a file</strong>
            </div>
          )}
        </div>
      </div>
      {palette && (
        <ProjectFilesPalette
          connection={connection}
          mode={palette}
          onClose={() => setPalette(undefined)}
          onMode={setPalette}
          onOpen={(path, line) => {
            setPalette(undefined);
            openFile(path, line);
          }}
          scope={workspace.scope}
          workspaceLabel={workspace.label}
        />
      )}
    </>
  );
  return variant === "task" ? (
    <aside
      aria-hidden={collapsed || undefined}
      aria-label="Project files"
      className="task-panel project-files-panel"
      inert={collapsed ? true : undefined}
      data-phone-page={phonePage}
      data-view={shown}
    >
      {onSplitRatio && splitRatio !== undefined && (
        <TaskPanelSplitter
          label="Resize Project files"
          onRatio={onSplitRatio}
          ratio={splitRatio}
        />
      )}
      {body}
    </aside>
  ) : (
    <section
      aria-label="Project files"
      className="project-files-panel"
      data-phone-page={phonePage}
      data-view={shown}
    >
      {body}
    </section>
  );
}
