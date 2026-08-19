import { Fragment, useEffect, useLayoutEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { ArrowLeft, Copy, FileCode2, FileText, Image, LoaderCircle, PanelRight, PanelRightClose, RefreshCw, Reply, X } from "lucide-react";
import type { FileViewerError, FileViewerSnapshot } from "@openaide/app-server-client";
import type { FileViewerTab } from "./useTaskFileViewer";
import { AgentMarkdown } from "./AgentMarkdown";
import { copyText } from "./clipboard";
import { ImagePreviewViewport } from "./ImagePreviewViewport";
import { highlightFileViewerLines } from "./fileViewerHighlight";
import { applyTaskPanelRatio, setLayoutResizing } from "./layoutResize";

export function FileViewerPanel({
  collapsed,
  onClose,
  onOpenFromHandle,
  onQuote,
  onRefresh,
  onSelect,
  onSplitRatio,
  onToggleCollapsed,
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
  onToggleCollapsed?: () => void;
  splitRatio: number;
  tab?: FileViewerTab;
  tabs: FileViewerTab[];
}) {
  const dragRef = useRef<{
    latest: number;
    pointerId: number;
    startRatio: number;
    startX: number;
    stack: HTMLElement;
    width: number;
    workbench?: HTMLElement;
  } | undefined>(undefined);
  const ratioRef = useRef(splitRatio);
  ratioRef.current = splitRatio;

  const beginResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const stack = event.currentTarget.closest(".task-work-stack");
    if (!(stack instanceof HTMLElement)) return;
    const workbench = stack.querySelector(".task-workbench");
    const width = stack.getBoundingClientRect().width;
    if (width <= 0) return;
    if (workbench instanceof HTMLElement) workbench.dataset.resizing = "true";
    stack.dataset.resizing = "true";
    setLayoutResizing(stack, true);
    dragRef.current = {
      latest: ratioRef.current,
      pointerId: event.pointerId,
      startRatio: ratioRef.current,
      startX: event.clientX,
      stack,
      width,
      workbench: workbench instanceof HTMLElement ? workbench : undefined,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const resizePanel = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const next = Math.min(0.72, Math.max(0.28, drag.startRatio - (event.clientX - drag.startX) / drag.width));
    drag.latest = next;
    applyTaskPanelRatio(drag.stack, next);
  };
  const finishResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    event.currentTarget.releasePointerCapture(event.pointerId);
    delete drag.workbench?.dataset.resizing;
    delete drag.stack.dataset.resizing;
    setLayoutResizing(drag.stack, false);
    dragRef.current = undefined;
    onSplitRatio(drag.latest);
  };

  useLayoutEffect(() => {
    const drag = dragRef.current;
    if (!drag) return;
    applyTaskPanelRatio(drag.stack, drag.latest);
    if (drag.workbench) drag.workbench.dataset.resizing = "true";
    drag.stack.dataset.resizing = "true";
    setLayoutResizing(drag.stack, true);
  });

  const [rawByHandle, setRawByHandle] = useState<Record<string, boolean>>({});
  const markdownRaw = Boolean(tab?.kind === "markdown" && tab.text && rawByHandle[tab.handle]);

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
        <div className="file-viewer-chrome">
        {onToggleCollapsed && !collapsed ? (
          <button
            aria-label="Back to Chat"
            className="file-viewer-back-to-chat"
            onClick={onToggleCollapsed}
            title="Back to Chat"
            type="button"
          >
            <ArrowLeft aria-hidden="true" size={16} />
            Chat
          </button>
        ) : null}
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
          <FileViewerChromeActions
            onRefresh={() => onRefresh(tab.handle)}
            tab={tab}
          />
        ) : null}
        </div>
        {tab ? (
          <ViewerBody
            markdownRaw={markdownRaw}
            onClose={onClose}
            onOpenFromHandle={onOpenFromHandle}
            onQuote={(text) => {
              onQuote(text);
              // Phone File Viewer covers Composer; return to Chat so the Quote is visible.
              if (!collapsed && onToggleCollapsed && narrowTaskPage()) onToggleCollapsed();
            }}
            onRawChange={(raw) => setRawByHandle((current) => ({ ...current, [tab.handle]: raw }))}
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

function FileViewerChromeActions({
  onRefresh,
  tab,
}: {
  onRefresh: () => void;
  tab: FileViewerTab;
}) {
  return (
    <div className="file-viewer-chrome-actions">
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
  markdownRaw,
  onClose,
  onOpenFromHandle,
  onQuote,
  onRawChange,
  onRefresh,
  tab,
}: {
  markdownRaw: boolean;
  onClose: (handle: string) => void;
  onOpenFromHandle: (handle: string, href: string) => void;
  onQuote: (text: string) => void;
  onRawChange: (raw: boolean) => void;
  onRefresh: (handle: string) => void;
  tab: FileViewerTab;
}) {
  const sourceTab = tab.kind === "markdown"
    ? { ...tab, language: tab.language ?? "md" }
    : tab;
  return (
    <div className="file-viewer-frame">
      <div className="file-viewer-header">
        <span className="file-viewer-path" title={tab.displayPath}>{tab.displayPath}</span>
        {tab.kind === "markdown" && tab.text ? (
          <MarkdownViewMode markdownRaw={markdownRaw} onRawChange={onRawChange} />
        ) : null}
      </div>
      <div className="file-viewer-body">
        {tab.kind === "pending" ? (
          <div className="file-viewer-fallback" data-kind="pending" role="status">
            <strong>Opening file</strong>
            <span>Reading a bounded snapshot.</span>
          </div>
        ) : null}
        {tab.truncated ? <p className="file-viewer-truncated">Showing the first 1 MiB. Refresh still uses that bound.</p> : null}
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
            <ImagePreviewViewport image={{ label: tab.preview.label, url: tab.preview.dataUrl }} />
          </div>
        ) : null}
        {((tab.kind === "source" || markdownRaw) && tab.text) ? (
          <SourceView onQuote={onQuote} tab={sourceTab} />
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
      <Reply aria-hidden="true" size={11} />
    </button>
  );
}

function narrowTaskPage() {
  return typeof globalThis.matchMedia === "function"
    && globalThis.matchMedia("(max-width: 760px)").matches;
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
