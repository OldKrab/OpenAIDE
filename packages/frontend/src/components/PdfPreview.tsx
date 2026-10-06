import { useEffect, useRef, useState } from "react";
import { Minus, Plus } from "lucide-react";
import type { FileViewerSnapshot } from "@openaide/app-server-client";
import { readFileViewerContent } from "../intents/fileViewerIntents";
import { useMermaidTheme } from "../mermaid/useMermaidTheme";
import { PDF_SOURCE_LIMIT_BYTES } from "../pdf/rendererProtocol";
import { openPdfViewerFrame, type PdfFrameFailure, type PdfViewerFrame } from "../pdf/viewerFrame";
import { postHostMessage } from "../services/hostBridge";
import { currentFrontendShell, type FileViewerContent, type FileViewerContentResult } from "../services/frontendShell";
import { sendWebviewTelemetry } from "../state/hostMessageTelemetry";

type PdfPreviewFailure = PdfFrameFailure | Exclude<FileViewerContentResult["kind"], "bytes"> | "unsupported";
type PdfPreviewState =
  | { phase: "loading" }
  | { phase: "ready"; pageCount: number }
  | { phase: "failed"; reason: PdfPreviewFailure };

const failureText: Record<PdfPreviewFailure, [title: string, detail: string]> = {
  unsupported: ["PDF preview unavailable", "This app cannot preview PDF files yet."],
  tooLarge: ["PDF too large to preview", "Previews are limited to 64 MiB. Download the file to read it."],
  notFound: ["File not found", "It may have moved or been deleted."],
  permissionDenied: ["Access denied", "Check file permissions and your connection."],
  notAFile: ["Not a regular file", "Only regular files can be previewed."],
  unavailable: ["PDF could not be loaded", "Check your connection and try again."],
  invalid: ["Invalid PDF", "The file is damaged or is not a PDF document."],
  encrypted: ["Password-protected PDF", "Encrypted documents cannot be previewed. Download the file to read it."],
  failed: ["PDF could not be rendered", "The document uses something this preview does not support."],
  renderer_unavailable: ["PDF preview unavailable", "The PDF renderer could not be loaded. Try again."],
  startup_timeout: ["PDF preview timed out", "The PDF renderer did not start. Try again."],
  load_timeout: ["PDF preview timed out", "The document took too long to open."],
};

const hostTelemetry = (event: string, fields: Record<string, unknown>) => sendWebviewTelemetry(postHostMessage, event, fields);

/**
 * Ephemeral presentation of one PDF snapshot: App Server only identifies the file, the
 * App Shell supplies its current bytes, and a sandboxed renderer draws them.
 */
export function PdfPreview({
  content = currentFrontendShell()?.fileViewerContent,
  onClose,
  onRefresh,
  tab,
  telemetry = hostTelemetry,
}: {
  content?: FileViewerContent;
  /** Injectable so tests can assert the start and terminal diagnostics. */
  telemetry?: (event: string, fields: Record<string, unknown>) => void;
  onClose: (handle: string) => void;
  onRefresh: (handle: string) => void;
  tab: FileViewerSnapshot;
}) {
  const host = useRef<HTMLDivElement>(null);
  const frame = useRef<PdfViewerFrame | undefined>(undefined);
  const theme = useMermaidTheme();
  const themeRef = useRef(theme);
  themeRef.current = theme;
  const [state, setState] = useState<PdfPreviewState>({ phase: "loading" });
  const [page, setPage] = useState(1);
  const [scale, setScale] = useState<{ fit: boolean; percent: number }>({ fit: true, percent: 100 });

  // A refreshed snapshot is a new object for the same handle, so it reloads the bytes.
  useEffect(() => {
    setState({ phase: "loading" });
    setPage(1);
    setScale({ fit: true, percent: 100 });
    const operationId = globalThis.crypto.randomUUID();
    const startedAt = performance.now();
    const controller = new AbortController();
    const finish = (next: Exclude<PdfPreviewState, { phase: "loading" }>, sourceBytes?: number) => {
      telemetry("pdf_preview_terminal", {
        operation_id: operationId,
        attempt: 1,
        duration_ms: Math.round(performance.now() - startedAt),
        outcome: controller.signal.aborted ? "cancelled" : next.phase === "ready" ? "success" : next.reason,
        page_count: next.phase === "ready" ? next.pageCount : undefined,
        source_bytes: sourceBytes,
      });
      if (!controller.signal.aborted) setState(next);
    };
    telemetry("pdf_preview_start", { operation_id: operationId, attempt: 1 });
    void (async () => {
      if (!content) return finish({ phase: "failed", reason: "unsupported" });
      const source = await readFileViewerContent(content, tab.handle, PDF_SOURCE_LIMIT_BYTES, controller.signal);
      if (source.kind !== "bytes") return finish({ phase: "failed", reason: source.kind });
      const sourceBytes = source.bytes.byteLength;
      if (controller.signal.aborted || !host.current) return finish({ phase: "failed", reason: "unavailable" }, sourceBytes);
      const opened = openPdfViewerFrame({
        bytes: source.bytes,
        host: host.current,
        onEvent: (event) => {
          if (event.type === "openaide.pdf.page") setPage(event.page);
          else setScale({ fit: event.fit, percent: event.percent });
        },
        theme: { background: themeRef.current.background, text: themeRef.current.text },
        title: `${tab.basename} preview`,
      });
      frame.current = opened;
      const result = await opened.loaded;
      if (result.kind === "loaded") return finish({ phase: "ready", pageCount: result.pageCount }, sourceBytes);
      if (frame.current === opened) frame.current = undefined;
      opened.dispose();
      finish({ phase: "failed", reason: result.reason }, sourceBytes);
    })();
    return () => {
      controller.abort();
      frame.current?.dispose();
      frame.current = undefined;
    };
  }, [content, tab, telemetry]);

  useEffect(() => {
    if (state.phase === "ready") frame.current?.setTheme({ background: theme.background, text: theme.text });
  }, [state.phase, theme.background, theme.text]);

  const ready = state.phase === "ready";
  return (
    <div className="file-viewer-pdf" data-phase={state.phase}>
      {ready ? (
        <div className="attachment-preview-chrome">
          <span className="file-viewer-preview-note" role="status">Page {Math.min(page, state.pageCount)} of {state.pageCount}</span>
          <div className="attachment-preview-actions">
            <button aria-label="Zoom PDF out" className="attachment-preview-action" onClick={() => frame.current?.zoom("out")} type="button">
              <Minus aria-hidden="true" size={16} />
            </button>
            <button aria-label="Fit PDF to width" className="attachment-preview-zoom" disabled={scale.fit} onClick={() => frame.current?.zoom("fit")} type="button">
              {scale.fit ? "Fit" : `${scale.percent}%`}
            </button>
            <button aria-label="Zoom PDF in" className="attachment-preview-action" onClick={() => frame.current?.zoom("in")} type="button">
              <Plus aria-hidden="true" size={16} />
            </button>
          </div>
        </div>
      ) : null}
      {state.phase === "loading" ? (
        <div className="file-viewer-fallback" data-kind="pending" role="status">
          <strong>Opening PDF</strong>
          <span>Loading the document preview.</span>
        </div>
      ) : null}
      {state.phase === "failed" ? (
        <div className="file-viewer-fallback" data-kind={state.reason === "unsupported" ? "binary" : "error"} role={state.reason === "unsupported" ? undefined : "alert"}>
          <strong>{failureText[state.reason][0]}</strong>
          <span>{failureText[state.reason][1]}</span>
          <div>
            {state.reason === "unsupported" ? null : (
              <button className="file-viewer-header-action" onClick={() => onRefresh(tab.handle)} type="button">Retry</button>
            )}
            <button className="file-viewer-header-action" onClick={() => onClose(tab.handle)} type="button">Close</button>
          </div>
        </div>
      ) : null}
      <div className="file-viewer-pdf-host" hidden={state.phase === "failed"} ref={host} />
    </div>
  );
}
