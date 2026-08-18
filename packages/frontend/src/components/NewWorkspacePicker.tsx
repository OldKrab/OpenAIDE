import { ArrowUp, ChevronRight, Folder, HardDrive, Pencil, RotateCw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { WorkspaceBrowserEntry, WorkspaceBrowserRoot, WorkspaceListDirectoryResult } from "@openaide/app-server-client";
import type { WorkspaceBrowserCallbacks } from "./appControllerCallbackTypes";
import { useBrowserRequestOwnership } from "./browserRequestOwnership";

type Directory = WorkspaceListDirectoryResult["directory"];

type Listing =
  | { kind: "roots"; roots: WorkspaceBrowserRoot[] }
  | { kind: "directory"; directory: Directory; entries: WorkspaceBrowserEntry[] };

type ExplorerState = {
  listing?: Listing;
  pending: boolean;
  error?: string;
};

/** Web Add Project explorer: path chrome, navigate-only rows, footer commit. */
export function NewWorkspacePicker({
  browser,
  onSelect,
}: {
  browser: WorkspaceBrowserCallbacks;
  onSelect: (workspace: { path: string; label: string }) => void;
}) {
  const [state, setState] = useState<ExplorerState>({ pending: true });
  const [pathDraft, setPathDraft] = useState("/");
  const [editingPath, setEditingPath] = useState(false);
  const [projectName, setProjectName] = useState("");
  const [editingName, setEditingName] = useState(false);
  const folderLabelRef = useRef<string | undefined>(undefined);
  const nameInput = useRef<HTMLInputElement>(null);
  const pathInput = useRef<HTMLInputElement>(null);
  const browserRef = useRef(browser);
  browserRef.current = browser;
  const requestOwnership = useBrowserRequestOwnership(browser.ownerKey);

  const showDirectory = (listing: WorkspaceListDirectoryResult) => {
    const nextLabel = listing.directory.label;
    const previousFolder = folderLabelRef.current;
    setState({
      listing: { kind: "directory", directory: listing.directory, entries: listing.entries },
      pending: false,
    });
    setPathDraft(listing.directory.path);
    setEditingPath(false);
    setProjectName((current) => {
      if (!previousFolder || current.trim() === previousFolder) return nextLabel;
      return current;
    });
    folderLabelRef.current = nextLabel;
  };

  const loadOwnedRoots = () => {
    const acceptsResult = requestOwnership.beginLatestRead();
    setState((current) => ({ listing: current.listing, pending: true }));
    void browserRef.current.listRoots().then(
      (roots) => {
        if (!acceptsResult()) return;
        const start = preferredWorkspaceStart(roots);
        if (start) {
          void browserRef.current.listDirectory(start).then(
            (listing) => {
              if (acceptsResult()) showDirectory(listing);
            },
            (error: unknown) => {
              if (acceptsResult()) setState((current) => ({ listing: current.listing, pending: false, error: errorMessage(error) }));
            },
          );
          return;
        }
        setState({ listing: { kind: "roots", roots }, pending: false });
        setPathDraft("/");
      },
      (error: unknown) => {
        if (acceptsResult()) setState((current) => ({ listing: current.listing, pending: false, error: errorMessage(error) }));
      },
    );
  };

  useEffect(() => {
    loadOwnedRoots();
    return requestOwnership.invalidateOwner;
  }, [browser.ownerKey]);

  useEffect(() => {
    if (editingPath) pathInput.current?.focus();
  }, [editingPath]);

  useEffect(() => {
    if (editingName) nameInput.current?.select();
  }, [editingName]);

  const openDirectory = (path: string) => {
    const acceptsResult = requestOwnership.beginLatestRead();
    setState((current) => ({ listing: current.listing, pending: true }));
    void browser.listDirectory(path).then(
      (listing) => {
        if (acceptsResult()) showDirectory(listing);
      },
      (error: unknown) => {
        if (acceptsResult()) setState((current) => ({ listing: current.listing, pending: false, error: errorMessage(error) }));
      },
    );
  };

  const listing = state.listing;
  const currentDirectory = listing?.kind === "directory" ? listing.directory : undefined;
  const canGoUp = Boolean(currentDirectory?.parentPath);
  const canAdd = listing?.kind === "directory" && !state.pending && Boolean(projectName.trim());

  const commitPath = () => {
    const next = pathDraft.trim() || "/";
    openDirectory(next);
  };

  return (
    <div className="project-folder-explorer">
      <div aria-busy={state.pending || undefined} className="project-folder-explorer-pane">
        <div className="project-folder-explorer-toolbar">
          <button
            aria-label="Go up"
            disabled={!canGoUp || state.pending}
            onClick={() => currentDirectory?.parentPath && openDirectory(currentDirectory.parentPath)}
            type="button"
          >
            <ArrowUp size={15} />
          </button>
          {editingPath ? (
            <form
              className="project-folder-explorer-path-form"
              onSubmit={(event) => {
                event.preventDefault();
                commitPath();
              }}
            >
              <input
                aria-label="Folder path"
                onBlur={() => setEditingPath(false)}
                onChange={(event) => setPathDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Escape") {
                    setPathDraft(currentDirectory?.path ?? "/");
                    setEditingPath(false);
                  }
                }}
                ref={pathInput}
                spellCheck={false}
                value={pathDraft}
              />
            </form>
          ) : (
            <nav aria-label="Folder path" className="project-folder-explorer-crumbs">
              {breadcrumbItems(listing).map((item, index, items) => (
                <span key={`${item.path}-${index}`}>
                  {index > 0 ? <span aria-hidden="true">›</span> : null}
                  <button
                    className={index === items.length - 1 ? "current" : undefined}
                    onClick={() => openDirectory(item.path)}
                    type="button"
                  >
                    {item.label}
                  </button>
                </span>
              ))}
            </nav>
          )}
          <button
            aria-label="Edit path"
            aria-pressed={editingPath}
            onClick={() => {
              setEditingPath(true);
              setPathDraft(currentDirectory?.path ?? (listing?.kind === "roots" ? "/" : pathDraft));
            }}
            type="button"
          >
            <Pencil size={14} />
          </button>
        </div>
        {state.pending ? <div aria-hidden="true" className="project-folder-explorer-progress" /> : <div className="project-folder-explorer-progress-slot" />}
        <div className="project-folder-explorer-list">
          {state.error && !listing ? (
            <div className="project-folder-explorer-status">
              <span>{state.error}</span>
              <button onClick={loadOwnedRoots} type="button"><RotateCw size={12} />Retry</button>
            </div>
          ) : null}
          {listing?.kind === "roots" ? listing.roots.map((root) => (
            <button className="project-folder-explorer-row" key={root.path} onClick={() => openDirectory(root.path)} type="button">
              <HardDrive size={15} />
              <span>{root.label}</span>
              <ChevronRight size={14} />
            </button>
          )) : null}
          {listing?.kind === "directory" ? listing.entries.map((entry) => (
            <button className="project-folder-explorer-row" key={entry.path} onClick={() => openDirectory(entry.path)} type="button">
              <Folder size={15} />
              <span>{entry.label}</span>
              <ChevronRight size={14} />
            </button>
          )) : null}
          {listing?.kind === "directory" && !listing.entries.length ? (
            <p className="project-folder-explorer-status">No folders in this directory.</p>
          ) : null}
        </div>
      </div>
      {state.error && listing ? <p className="project-folder-explorer-notice" role="alert">{state.error}</p> : null}
      <footer className="project-folder-explorer-footer">
        <p className="project-folder-explorer-name">
          Project name:
          {listing?.kind === "directory" ? (
            editingName ? (
              <input
                aria-label="Project name"
                onBlur={() => setEditingName(false)}
                onChange={(event) => setProjectName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    setEditingName(false);
                  }
                  if (event.key === "Escape") {
                    setProjectName(currentDirectory?.label ?? "");
                    folderLabelRef.current = currentDirectory?.label ?? "";
                    setEditingName(false);
                  }
                }}
                ref={nameInput}
                spellCheck={false}
                value={projectName}
              />
            ) : (
              <button onClick={() => setEditingName(true)} type="button">{projectName || currentDirectory?.label}</button>
            )
          ) : (
            <span>Select a folder</span>
          )}
        </p>
        <button
          className="project-folder-explorer-add"
          disabled={!canAdd}
          onClick={() => currentDirectory && onSelect({
            path: currentDirectory.path,
            label: projectName.trim() || currentDirectory.label,
          })}
          type="button"
        >
          Add this folder
        </button>
      </footer>
    </div>
  );
}

export function preferredWorkspaceStart(roots: WorkspaceBrowserRoot[]) {
  const home = roots.find((root) => isHomeDirectory(root.path));
  if (home) return home.path;
  const nested = roots.find((root) => !isFilesystemRoot(root.path));
  return nested?.path ?? roots[0]?.path;
}

function isFilesystemRoot(path: string) {
  return path === "/" || /^[A-Za-z]:[\\/]?$/.test(path);
}

function isHomeDirectory(path: string) {
  const normalized = path.replaceAll("\\", "/").replace(/\/$/, "");
  return /(?:^|\/)home\/[^/]+$/.test(normalized) || /(?:^|\/)Users\/[^/]+$/.test(normalized);
}

function breadcrumbItems(listing: Listing | undefined) {
  if (!listing || listing.kind === "roots") return [{ label: "/", path: "/" }];
  const path = listing.directory.path;
  const separator = path.includes("\\") ? "\\" : "/";
  if (isFilesystemRoot(path)) return [{ label: path.replace(/[\\/]+$/, "") || "/", path }];
  const drive = /^[A-Za-z]:/.exec(path);
  const rest = (drive ? path.slice(drive[0].length) : path).split(/[\\/]/).filter(Boolean);
  const items = drive
    ? [{ label: drive[0], path: `${drive[0]}${separator}` }]
    : [{ label: "/", path: "/" }];
  let acc = items[0]!.path;
  for (const segment of rest) {
    acc = acc.endsWith(separator) ? `${acc}${segment}` : `${acc}${separator}${segment}`;
    items.push({ label: segment, path: acc });
  }
  return items;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Workspace browser failed.";
}
