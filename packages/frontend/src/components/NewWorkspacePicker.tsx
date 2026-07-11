import { ArrowLeft, Check, Folder, HardDrive, RotateCw } from "lucide-react";
import { useEffect, useState } from "react";
import type { WorkspaceBrowserEntry, WorkspaceBrowserRoot, WorkspaceListDirectoryResult } from "@openaide/app-server-client";
import type { WorkspaceBrowserCallbacks } from "./appControllerCallbackTypes";

type Directory = WorkspaceListDirectoryResult["directory"];

type PickerState =
  | { status: "loading" }
  | { status: "roots"; roots: WorkspaceBrowserRoot[] }
  | { status: "directory"; directory: Directory; entries: WorkspaceBrowserEntry[]; history: Directory[] }
  | { status: "error"; message: string };

export function NewWorkspacePicker({
  browser,
  onSelect,
}: {
  browser: WorkspaceBrowserCallbacks;
  onSelect: (workspace: { path: string; label: string }) => void;
}) {
  const [state, setState] = useState<PickerState>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    void loadRoots(browser, (next) => {
      if (!cancelled) setState(next);
    });
    return () => {
      cancelled = true;
    };
  }, [browser]);

  const openDirectory = (path: string) => {
    const history = state.status === "directory" ? [...state.history, state.directory] : [];
    setState({ status: "loading" });
    void browser.listDirectory(path).then(
      (listing) => setState({ status: "directory", directory: listing.directory, entries: listing.entries, history }),
      (error: unknown) => setState({ status: "error", message: errorMessage(error) }),
    );
  };

  const goBack = () => {
    if (state.status !== "directory") return;
    const previous = state.history[state.history.length - 1];
    if (!previous) {
      void loadRoots(browser, setState);
      return;
    }
    const history = state.history.slice(0, -1);
    setState({ status: "loading" });
    void browser.listDirectory(previous.path).then(
      (listing) => setState({ status: "directory", directory: listing.directory, entries: listing.entries, history }),
      (error: unknown) => setState({ status: "error", message: errorMessage(error) }),
    );
  };

  if (state.status === "loading") {
    return <div className="new-workspace-picker-status">Loading folders</div>;
  }
  if (state.status === "error") {
    return (
      <div className="new-workspace-picker-status">
        <span>{state.message}</span>
        <button onClick={() => void loadRoots(browser, setState)} type="button">
          <RotateCw size={12} />
          Retry
        </button>
      </div>
    );
  }
  if (state.status === "roots") {
    return (
      <div className="new-workspace-picker" role="none">
        <div className="new-workspace-picker-title">Browse folders</div>
        {state.roots.map((root) => (
          <button className="new-workspace-picker-row" key={root.path} onClick={() => openDirectory(root.path)} type="button">
            <HardDrive size={13} />
            <span>{root.label}</span>
          </button>
        ))}
      </div>
    );
  }

  return (
    <div className="new-workspace-picker" role="none">
      <div className="new-workspace-picker-title">{state.directory.label}</div>
      <button className="new-workspace-picker-row" onClick={goBack} type="button">
        <ArrowLeft size={13} />
        <span>Back</span>
      </button>
      <button
        className="new-workspace-picker-row choose"
        onClick={() => onSelect({ path: state.directory.path, label: state.directory.label })}
        type="button"
      >
        <Check size={13} />
        <span>Use this folder</span>
      </button>
      {state.entries.map((entry) => (
        <button className="new-workspace-picker-row" key={entry.path} onClick={() => openDirectory(entry.path)} type="button">
          <Folder size={13} />
          <span>{entry.label}</span>
        </button>
      ))}
    </div>
  );
}

async function loadRoots(
  browser: WorkspaceBrowserCallbacks,
  setState: (state: PickerState) => void,
) {
  setState({ status: "loading" });
  try {
    const roots = await browser.listRoots();
    setState({ status: "roots", roots });
  } catch (error) {
    setState({ status: "error", message: errorMessage(error) });
  }
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Workspace browser failed.";
}
