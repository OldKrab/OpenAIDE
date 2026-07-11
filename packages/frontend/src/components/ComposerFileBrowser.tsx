import { ArrowLeft, FileImage, FileText, Folder, RotateCw } from "lucide-react";
import { useEffect, useState } from "react";
import type {
  AttachmentListDirectoryResult,
  FileBrowserEntry,
  FileBrowserEntryId,
  FileBrowserRoot,
  FileBrowserRootId,
} from "@openaide/app-server-client";
import type { TaskFileBrowserCallbacks } from "./appControllerCallbackTypes";

type DirectoryRef = AttachmentListDirectoryResult["directory"];

type BrowserState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | {
      status: "ready";
      directory: DirectoryRef;
      entries: FileBrowserEntry[];
    };

export function ComposerFileBrowser({
  browser,
  onAttached,
}: {
  browser: TaskFileBrowserCallbacks;
  onAttached: () => void;
}) {
  const [state, setState] = useState<BrowserState>({ status: "loading" });
  const [history, setHistory] = useState<DirectoryRef[]>([]);
  const [pendingEntryId, setPendingEntryId] = useState<string | undefined>();
  const [slowLoading, setSlowLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void loadRoots(browser, setState, () => cancelled);
    return () => {
      cancelled = true;
    };
  }, [browser]);

  useEffect(() => {
    if (state.status !== "loading") {
      setSlowLoading(false);
      return undefined;
    }
    setSlowLoading(false);
    const timeout = globalThis.setTimeout(() => setSlowLoading(true), 1200);
    return () => globalThis.clearTimeout(timeout);
  }, [state.status]);

  const openDirectory = (rootId: FileBrowserRootId, directoryId?: FileBrowserEntryId) => {
    if (state.status === "ready") {
      setHistory((current) => [...current, state.directory]);
    }
    setState({ status: "loading" });
    void browser.listDirectory(rootId, directoryId).then(
      (listing) =>
        setState({
          status: "ready",
          directory: listing.directory,
          entries: listing.entries,
        }),
      (error: unknown) => setState({ status: "error", message: errorMessage(error) }),
    );
  };

  const openPreviousDirectory = () => {
    const previous = history[history.length - 1];
    if (!previous) return;
    setHistory((current) => current.slice(0, -1));
    setState({ status: "loading" });
    void browser.listDirectory(previous.rootId, previous.directoryId ?? undefined).then(
      (listing) =>
        setState({
          status: "ready",
          directory: listing.directory,
          entries: listing.entries,
        }),
      (error: unknown) => setState({ status: "error", message: errorMessage(error) }),
    );
  };

  const attach = (entry: FileBrowserEntry, mode: "reference" | "embedded") => {
    setPendingEntryId(entry.entryId);
    const request =
      mode === "reference"
        ? browser.attachFileReference(entry.entryId)
        : browser.attachEmbedded(entry.entryId);
    void request.then(
      () => {
        setPendingEntryId(undefined);
        onAttached();
      },
      (error: unknown) => {
        setPendingEntryId(undefined);
        setState({ status: "error", message: errorMessage(error) });
      },
    );
  };

  if (state.status === "loading") {
    return (
      <div className="composer-file-browser-status">
        <span>{slowLoading ? "Still loading workspace files" : "Loading files"}</span>
        {slowLoading ? <small>Waiting for App Server file listing.</small> : null}
      </div>
    );
  }
  if (state.status === "error") {
    return (
      <div className="composer-file-browser-status">
        <span>{state.message}</span>
        <button onClick={() => void loadRoots(browser, setState)} type="button">
          <RotateCw size={12} />
          Retry
        </button>
      </div>
    );
  }

  const entries = state.entries;
  return (
    <div className="composer-file-browser">
      <div className="composer-file-browser-title">{state.directory.label}</div>
      {history.length > 0 ? (
        <button className="composer-file-row" onClick={openPreviousDirectory} type="button">
          <ArrowLeft size={13} />
          <span>Back</span>
        </button>
      ) : null}
      {entries.length === 0 ? (
        <div className="composer-file-browser-empty">No files here.</div>
      ) : null}
      {entries.map((entry) =>
        entry.kind === "directory" ? (
          <button
            className="composer-file-row"
            key={entry.entryId}
            onClick={() => openDirectory(state.directory.rootId, entry.entryId)}
            type="button"
          >
            <Folder size={13} />
            <span>{entry.label}</span>
          </button>
        ) : (
          <div className="composer-file-row file" key={entry.entryId}>
            {isImageFileLabel(entry.label) ? <FileImage size={13} /> : <FileText size={13} />}
            <span>{entry.label}</span>
            <div className="composer-file-row-actions">
              <button
                disabled={!entry.selectable || pendingEntryId === entry.entryId}
                onClick={() => attach(entry, "reference")}
                type="button"
              >
                Reference
              </button>
              <button
                disabled={!entry.selectable || pendingEntryId === entry.entryId}
                onClick={() => attach(entry, "embedded")}
                type="button"
              >
                Embed
              </button>
            </div>
          </div>
        ),
      )}
    </div>
  );
}

function isImageFileLabel(label: string) {
  return /\.(avif|bmp|gif|heic|heif|jpe?g|png|svg|webp)$/i.test(label);
}

async function loadRoots(
  browser: TaskFileBrowserCallbacks,
  setState: (state: BrowserState) => void,
  cancelled: () => boolean = () => false,
) {
  setState({ status: "loading" });
  try {
    const roots = await browser.listRoots();
    if (cancelled()) return;
    const firstRoot = roots[0];
    if (!firstRoot) {
      setState({ status: "error", message: "No file roots available." });
      return;
    }
    const listing = await browser.listDirectory(firstRoot.rootId);
    if (cancelled()) return;
    setState({
      status: "ready",
      directory: listing.directory,
      entries: listing.entries,
    });
  } catch (error) {
    if (!cancelled()) setState({ status: "error", message: errorMessage(error) });
  }
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "File browser failed.";
}
