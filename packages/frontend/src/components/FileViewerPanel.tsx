import { useMemo, useState, type ComponentProps, type ReactNode } from "react";
import { Copy, Download, FileCode2, FileText, FileType2, Image, LoaderCircle, RefreshCw, X } from "lucide-react";
import type { FileViewerError, FileViewerSnapshot } from "@openaide/app-server-client";
import type { FileViewerTab } from "./useTaskFileViewer";
import { FileCodeReader } from "./FileCodeReader";
import { AgentMarkdown } from "./AgentMarkdown";
import { copyText } from "./clipboard";
import { ImagePreviewViewport } from "./ImagePreviewViewport";
import { PdfPreview } from "./PdfPreview";
import { currentFrontendShell, type FileViewerDownloads } from "../services/frontendShell";
import { useFileViewerDownload } from "./useFileViewerDownload";

/** What the owner lets the reader do with one file's lines. */
export type FileReaderNotes = Pick<
  ComponentProps<typeof FileCodeReader>,
  "notes" | "onSaveNote" | "onRemoveNote" | "submitLabel"
> & { path: string };

/** Opened files of one workspace: File Tabs, per-file actions and the preview body. The owner
 * places it; it holds no layout of its own beyond filling the space it is given. */
export function FileViewerPanel({
  downloads = currentFrontendShell()?.fileViewerDownloads,
  headerAction,
  onClose,
  onOpenFromHandle,
  onRefresh,
  onSelect,
  readerNotes,
  tab,
  tabs,
}: {
  downloads?: FileViewerDownloads;
  /** Extra action beside the path of the active file, such as a link to its diff. */
  headerAction?: ReactNode;
  onClose: (handle: string) => void;
  onOpenFromHandle: (handle: string, href: string) => void;
  onRefresh: (handle: string) => void;
  onSelect: (handle: string) => void;
  /** Notes and comment actions for a file; absent leaves source read-only. */
  readerNotes?: (displayPath: string) => FileReaderNotes | undefined;
  tab?: FileViewerTab;
  tabs: FileViewerTab[];
}) {
  const download = useFileViewerDownload(tab, downloads);
  const [rawByHandle, setRawByHandle] = useState<Record<string, boolean>>({});
  const markdownRaw = Boolean(tab?.kind === "markdown" && tab.text && rawByHandle[tab.handle]);

  if (tabs.length === 0) return null;
  return (
    <div aria-label="File Viewer" className="file-viewer" role="group">
        <div className="file-viewer-chrome">
        <div className="file-viewer-tabs" role="tablist" aria-label="File Tabs">
          {tabs.map((item) => (
            <div className="file-viewer-tab" data-selected={item.handle === tab?.handle} key={item.handle}>
              <button aria-selected={item.handle === tab?.handle} onClick={() => onSelect(item.handle)} role="tab" type="button">
                {tabIcon(item)}
                <span className="file-viewer-tab-name" title={item.displayPath}>{item.basename}</span>
              </button>
              <button aria-label={`Close ${item.basename}`} className="file-viewer-tab-close" onClick={() => onClose(item.handle)} type="button">
                <X size={12} />
              </button>
            </div>
          ))}
        </div>
        {tab ? (
          <FileViewerChromeActions
            download={download}
            onRefresh={() => onRefresh(tab.handle)}
            tab={tab}
          />
        ) : null}
        </div>
        {download.available && download.error ? (
          <div className="file-viewer-download-error" role="alert">
            <span>{download.error}</span>
            <button aria-label="Retry download" className="file-viewer-header-action" disabled={download.disabled} onClick={download.start} type="button">Retry</button>
          </div>
        ) : null}
        {tab ? (
          <ViewerBody
            headerAction={headerAction}
            markdownRaw={markdownRaw}
            onClose={onClose}
            onOpenFromHandle={onOpenFromHandle}
            onRawChange={(raw) => setRawByHandle((current) => ({ ...current, [tab.handle]: raw }))}
            onRefresh={onRefresh}
            reader={readerNotes?.(tab.displayPath)}
            tab={tab}
          />
        ) : <div className="file-viewer-fallback">No file selected.</div>}
    </div>
  );
}

function FileViewerChromeActions({
  download,
  onRefresh,
  tab,
}: {
  download: ReturnType<typeof useFileViewerDownload>;
  onRefresh: () => void;
  tab: FileViewerTab;
}) {
  return (
    <div className="file-viewer-chrome-actions">
      {download.available ? (
        <button aria-label="Download file" aria-busy={download.busy} className="file-viewer-icon-btn" disabled={download.disabled} onClick={download.start} title="Download file" type="button">
          {download.busy ? <LoaderCircle size={13} /> : <Download size={13} />}
        </button>
      ) : null}
      <button
        aria-label="Copy path"
        className="file-viewer-icon-btn"
        onClick={() => {
          void copyText(tab.displayPath);
        }}
        title="Copy path"
        type="button"
      >
        <Copy size={13} />
      </button>
      <button aria-label="Refresh snapshot" className="file-viewer-icon-btn" disabled={tab.kind === "pending"} onClick={onRefresh} title="Refresh" type="button">
        <RefreshCw size={13} />
      </button>
    </div>
  );
}

function MarkdownViewMode({
  markdownRaw,
  onRawChange,
}: {
  markdownRaw: boolean;
  onRawChange: (raw: boolean) => void;
}) {
  return (
    <div className="file-viewer-view-mode" role="group" aria-label="Markdown view">
      <button
        aria-label="Show Markdown preview"
        aria-pressed={!markdownRaw}
        className="file-viewer-header-action"
        onClick={() => onRawChange(false)}
        type="button"
      >
        Preview
      </button>
      <button
        aria-label="Show raw Markdown"
        aria-pressed={markdownRaw}
        className="file-viewer-header-action"
        onClick={() => onRawChange(true)}
        type="button"
      >
        Raw
      </button>
    </div>
  );
}

function ViewerBody({
  headerAction,
  markdownRaw,
  onClose,
  onOpenFromHandle,
  onRawChange,
  onRefresh,
  reader,
  tab,
}: {
  headerAction?: ReactNode;
  markdownRaw: boolean;
  onClose: (handle: string) => void;
  onOpenFromHandle: (handle: string, href: string) => void;
  onRawChange: (raw: boolean) => void;
  onRefresh: (handle: string) => void;
  reader?: FileReaderNotes;
  tab: FileViewerTab;
}) {
  return (
    <div className="file-viewer-frame">
      <div className="file-viewer-header">
        {/* Inside a workspace the path reads from its root; the full path stays in the tooltip. */}
        <span className="file-viewer-path" title={tab.displayPath}>{reader?.path ?? tab.displayPath}</span>
        {tab.kind === "markdown" && tab.text ? (
          <MarkdownViewMode markdownRaw={markdownRaw} onRawChange={onRawChange} />
        ) : null}
        {headerAction}
      </div>
      <div className="file-viewer-body">
        {tab.kind === "pending" ? (
          <div className="file-viewer-fallback" data-kind="pending" role="status">
            <strong>Opening file</strong>
            <span>Reading a bounded snapshot.</span>
          </div>
        ) : null}
        {tab.truncated && tab.kind !== "image" ? <p className="file-viewer-truncated">Showing the first 1 MiB. Refresh still uses that bound.</p> : null}
        {tab.kind === "markdown" && tab.text && !markdownRaw ? (
          <div className="file-viewer-markdown">
            <AgentMarkdown
              onOpenRelativeHref={(href) => onOpenFromHandle(tab.handle, href)}
              renderDiagrams={false}
              text={tab.text}
            />
          </div>
        ) : null}
        {tab.kind === "image" && tab.preview ? (
          <div className="file-viewer-image">
            <ImagePreviewViewport
              image={{ label: tab.preview.label, url: tab.preview.dataUrl }}
              toolbarActions={tab.truncated ? (
                <span className="file-viewer-preview-note" title="Resized preview; animations show their first frame.">Reduced image preview</span>
              ) : undefined}
            />
          </div>
        ) : null}
        {tab.kind === "pdf" ? (
          <PdfPreview onClose={onClose} onRefresh={onRefresh} tab={tab} />
        ) : null}
        {tab.kind === "source" && tab.text ? (
          <SourceView reader={reader} tab={tab} />
        ) : null}
        {tab.kind === "markdown" && markdownRaw && tab.text ? (
          <SourceView reader={reader} tab={{ ...tab, language: tab.language ?? "md" }} />
        ) : null}
        {tab.kind === "error" ? (
          <div className="file-viewer-fallback" data-kind="error" role="alert">
            <strong>{errorLabel(tab.error)}</strong>
            <span>This File Tab stays open. Composer is unchanged.</span>
            <div>
              <button className="file-viewer-header-action" onClick={() => onRefresh(tab.handle)} type="button">Retry</button>
              <button className="file-viewer-header-action" onClick={() => onClose(tab.handle)} type="button">Close</button>
            </div>
          </div>
        ) : null}
        {tab.kind === "binary" ? (
          <div className="file-viewer-fallback" data-kind="binary">
            <strong>Unsupported file</strong>
            <span>{tab.basename} is not UTF-8 text. v1 shows this fallback instead of a hex view.</span>
            <button className="file-viewer-header-action" onClick={() => onClose(tab.handle)} type="button">Close</button>
          </div>
        ) : null}
      </div>
    </div>
  );
}

function SourceView({ reader, tab }: { reader?: FileReaderNotes; tab: FileViewerSnapshot }) {
  const lines = useMemo(() => (tab.text ?? "").split("\n").map((text, index) => ({ kind: "context", text, oldLine: null, newLine: index + 1 })), [tab.text]);
  return (
    <FileCodeReader
      key={tab.handle}
      lines={lines}
      language={tab.language}
      focusLine={tab.focusLine}
      notes={reader?.notes}
      onRemoveNote={reader?.onRemoveNote}
      onSaveNote={reader?.onSaveNote}
      path={reader?.path ?? tab.displayPath}
      submitLabel={reader?.submitLabel}
    />
  );
}

function tabIcon(tab: FileViewerTab) {
  if (tab.kind === "pending") return <LoaderCircle className="file-viewer-pending-icon" size={12} />;
  if (tab.kind === "markdown") return <FileText size={12} />;
  if (tab.kind === "image") return <Image size={12} />;
  if (tab.kind === "pdf") return <FileType2 size={12} />;
  return <FileCode2 size={12} />;
}

function errorLabel(error?: FileViewerError | null) {
  if (error === "permissionDenied") return "Permission denied";
  if (error === "notFound") return "File not found";
  if (error === "notAFile") return "Not a file";
  if (error === "unsupported") return "This file cannot be previewed";
  return "Unable to read file";
}
