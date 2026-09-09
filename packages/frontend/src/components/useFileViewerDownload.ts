import { useEffect, useRef, useState } from "react";
import { downloadFileViewer } from "../intents/fileViewerIntents";
import type { FileViewerDownloads, FileViewerDownloadResult } from "../services/frontendShell";
import type { FileViewerTab } from "./useTaskFileViewer";

const failureText = {
  notFound: "File not found. It may have moved or been deleted.",
  permissionDenied: "Access denied. Check file permissions and your connection.",
  notAFile: "Only regular files can be downloaded.",
  unavailable: "Download could not start. Check your connection and try again.",
};

/** Pending presentation belongs to this tab; leaving it cancels an unstarted download. */
export function useFileViewerDownload(tab: FileViewerTab | undefined, downloads: FileViewerDownloads | undefined) {
  const pending = useRef<AbortController | undefined>(undefined);
  const [state, setState] = useState<{ handle: string; result: FileViewerDownloadResult | "pending" }>();
  useEffect(() => () => {
    pending.current?.abort();
    pending.current = undefined;
  }, [tab?.handle, downloads]);
  const result = state?.handle === tab?.handle ? state?.result : undefined;
  const busy = result === "pending" && pending.current !== undefined;
  const error = result && result !== "pending" && result !== "started" ? failureText[result] : undefined;
  return {
    busy,
    error,
    available: Boolean(downloads),
    disabled: !tab || tab.kind === "pending" || tab.handle.startsWith("pending:") || busy,
    async start() {
      if (!downloads || !tab || pending.current || tab.kind === "pending" || tab.handle.startsWith("pending:")) return;
      const controller = new AbortController();
      pending.current = controller;
      setState({ handle: tab.handle, result: "pending" });
      const result = await downloadFileViewer(downloads, tab.handle, tab.basename, controller.signal);
      if (controller.signal.aborted) return;
      pending.current = undefined;
      setState({ handle: tab.handle, result });
    },
  };
}
