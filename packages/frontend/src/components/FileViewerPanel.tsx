import { Fragment, useEffect, useLayoutEffect, useMemo, useRef, type PointerEvent as ReactPointerEvent } from "react";
import { Copy, FileCode2, FileText, Image, LoaderCircle, PanelRight, PanelRightClose, RefreshCw, Reply, X } from "lucide-react";
import type { FileViewerError, FileViewerSnapshot } from "@openaide/app-server-client";
import type { FileViewerTab } from "./useTaskFileViewer";
import { AgentMarkdown } from "./AgentMarkdown";
import { copyText } from "./clipboard";
import { ImagePreviewViewport } from "./ImagePreviewViewport";
import { highlightFileViewerLines } from "./fileViewerHighlight";
import { setLayoutResizing } from "./layoutResize";

export function FileViewerPanel({
  collapsed,
  onClose,
  onOpenFromHandle,
  onQuote,
  onRefresh,
  onSelect,
  onSplitRatio,
  splitRatio,
  tab,
  tabs,
}: {
  collapsed: boolean;
  onClose: (handle: string) => void;
  onOpenFromHandle: (handle: string, href: string) => void;
  onQuote: (text: string) => void;
  onRefresh: (handle: string) => void;
  onSelect: (handle: string) => void;
  onSplitRatio: (ratio: number) => void;
  splitRatio: number;
  tab?: FileViewerTab;
  tabs: FileViewerTab[];
}) {
  const dragRef = useRef<{
    latest: number;
    pointerId: number;
    startRatio: number;
    startX: number;
    width: number;
    workbench: HTMLElement;
  } | undefined>(undefined);
  const ratioRef = useRef(splitRatio);
  ratioRef.current = splitRatio;

  const beginResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const workbench = event.currentTarget.closest(".task-workbench");
    if (!(workbench instanceof HTMLElement)) return;
    const width = workbench.getBoundingClientRect().width;
    if (width <= 0) return;
    workbench.dataset.resizing = "true";
    setLayoutResizing(workbench.closest(".task-work-stack"), true);
    dragRef.current = {
      latest: ratioRef.current,
      pointerId: event.pointerId,
      startRatio: ratioRef.current,
      startX: event.clientX,
      width,
      workbench,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const resizePanel = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const next = Math.min(0.72, Math.max(0.28, drag.startRatio - (event.clientX - drag.startX) / drag.width));
    drag.latest = next;
    drag.workbench.style.setProperty("--task-panel-ratio", String(next));
  };
  const finishResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    event.currentTarget.releasePointerCapture(event.pointerId);
    delete drag.workbench.dataset.resizing;
    setLayoutResizing(drag.workbench.closest(".task-work-stack"), false);
    dragRef.current = undefined;
    onSplitRatio(drag.latest);
  };

  useLayoutEffect(() => {
    const drag = dragRef.current;
    if (!drag) return;
    drag.workbench.style.setProperty("--task-panel-ratio", String(drag.latest));
    drag.workbench.dataset.resizing = "true";
    setLayoutResizing(drag.workbench.closest(".task-work-stack"), true);
  });

  if (tabs.length === 0) return null;
  return (
    <aside aria-hidden={collapsed} aria-label="File Viewer" className="task-panel file-viewer">
        <div
          aria-hidden={collapsed}
          aria-label="Resize File Viewer"
          aria-orientation="vertical"
          className="task-panel-splitter"
          onPointerCancel={finishResize}
          onPointerDown={beginResize}
          onPointerMove={resizePanel}
          onPointerUp={finishResize}
          role="separator"
          tabIndex={collapsed ? -1 : 0}
        />
        <div className="file-viewer-tabs" role="tablist" aria-label="File Tabs">
          {tabs.map((item) => (
            <div className="file-viewer-tab" data-selected={item.handle === tab?.handle} key={item.handle}>
              <button aria-selected={item.handle === tab?.handle} onClick={() => onSelect(item.handle)} role="tab" type="button">
                {tabIcon(item)}
                {item.basename}
              </button>
              <button aria-label={`Close ${item.basename}`} className="file-viewer-tab-close" onClick={() => onClose(item.handle)} type="button">
                <X size={12} />
              </button>
            </div>
          ))}
        </div>
        {tab ? (
          <ViewerBody
            onClose={onClose}
            onOpenFromHandle={onOpenFromHandle}
            onQuote={onQuote}
            onRefresh={onRefresh}
            tab={tab}
          />
        ) : <div className="file-viewer-fallback">No file selected.</div>}
    </aside>
  );
}

export function TaskPanelToggle({
  collapsed,
  visible,
  onToggle,
}: {
  collapsed: boolean;
  onToggle: () => void;
  visible: boolean;
}) {
  if (!visible) return null;
  return (
    <button
      aria-label={collapsed ? "Show Task Panel" : "Hide Task Panel"}
      aria-pressed={!collapsed}
      className="task-panel-toggle"
      onClick={onToggle}
      title={collapsed ? "Show Task Panel" : "Hide Task Panel"}
      type="button"
    >
      {collapsed ? <PanelRight size={16} /> : <PanelRightClose size={16} />}
    </button>
  );
}

function ViewerBody({
  onClose,
  onOpenFromHandle,
  onQuote,
  onRefresh,
  tab,
}: {
  onClose: (handle: string) => void;
  onOpenFromHandle: (handle: string, href: string) => void;
  onQuote: (text: string) => void;
  onRefresh: (handle: string) => void;
  tab: FileViewerTab;
}) {
  return (
    <div className="file-viewer-frame">
      <div className="file-viewer-header">
        <span className="file-viewer-path" title={tab.displayPath}>{tab.displayPath}</span>
        <div className="file-viewer-header-actions">
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
          <button aria-label="Refresh snapshot" className="file-viewer-icon-btn" disabled={tab.kind === "pending"} onClick={() => onRefresh(tab.handle)} title="Refresh" type="button">
            <RefreshCw size={13} />
          </button>
        </div>
      </div>
      <div className="file-viewer-body">
        {tab.kind === "pending" ? (
          <div className="file-viewer-fallback" data-kind="pending" role="status">
            <strong>Opening file</strong>
            <span>Reading a bounded snapshot.</span>
          </div>
        ) : null}
        {tab.truncated ? <p className="file-viewer-truncated">Showing the first 1 MiB. Refresh still uses that bound.</p> : null}
        {tab.kind === "markdown" && tab.text ? (
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
            <ImagePreviewViewport image={{ label: tab.preview.label, url: tab.preview.dataUrl }} />
          </div>
        ) : null}
        {(tab.kind === "source") && tab.text ? (
          <SourceView onQuote={onQuote} tab={tab} />
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

function SourceView({ onQuote, tab }: { onQuote: (text: string) => void; tab: FileViewerSnapshot }) {
  const text = tab.text ?? "";
  const lines = useMemo(() => text.split("\n"), [text]);
  const highlighted = useMemo(() => highlightFileViewerLines(text, tab.language), [tab.language, text]);
  useEffect(() => {
    if (!tab.focusLine) return;
    document.querySelector(".file-viewer-line[data-focus='true']")?.scrollIntoView({ block: "center" });
  }, [tab.focusLine, tab.handle]);
  return (
    <div className="file-viewer-source" data-language={tab.language}>
      {lines.map((line, index) => {
        const lineNumber = index + 1;
        return (
          <div className="file-viewer-line" data-focus={tab.focusLine === lineNumber} key={lineNumber}>
            <span className="file-viewer-gutter">
              {lineNumber}
              <QuoteLineButton displayPath={tab.displayPath} lineNumber={lineNumber} lineText={line} onQuote={onQuote} />
            </span>
            <span className="file-viewer-code">
              {highlighted[index]?.map((span, spanIndex) => (
                span.className
                  ? <span className={span.className} key={spanIndex}>{span.text}</span>
                  : <Fragment key={spanIndex}>{span.text}</Fragment>
              ))}
            </span>
          </div>
        );
      })}
    </div>
  );
}

function QuoteLineButton({
  displayPath,
  lineNumber,
  lineText,
  onQuote,
}: {
  displayPath: string;
  lineNumber: number;
  lineText: string;
  onQuote: (text: string) => void;
}) {
  return (
    <button
      aria-label={`Quote line ${lineNumber}`}
      className="file-viewer-quote"
      onClick={() => onQuote(`${displayPath}:${lineNumber}\n${lineText}`)}
      title="Quote line"
      type="button"
    >
      <Reply size={11} />
    </button>
  );
}

function tabIcon(tab: FileViewerTab) {
  if (tab.kind === "pending") return <LoaderCircle className="file-viewer-pending-icon" size={12} />;
  if (tab.kind === "markdown") return <FileText size={12} />;
  if (tab.kind === "image") return <Image size={12} />;
  return <FileCode2 size={12} />;
}

function errorLabel(error?: FileViewerError | null) {
  if (error === "permissionDenied") return "Permission denied";
  if (error === "notFound") return "File not found";
  if (error === "notAFile") return "Not a file";
  if (error === "unsupported") return "This file cannot be previewed";
  return "Unable to read file";
}
