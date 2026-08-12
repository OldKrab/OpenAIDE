import { Code2, Copy, ImageIcon, LoaderCircle, Maximize2, RotateCcw } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { AttachmentImagePreviewLightbox } from "./AttachmentImagePreview";
import { ImagePreviewViewport } from "./ImagePreviewViewport";
import { copyText } from "./clipboard";
import { renderMermaidDiagram, type MermaidDiagramResult } from "../mermaid/renderService";
import { useMermaidTheme } from "../mermaid/useMermaidTheme";

type CopyState = "idle" | "copied" | "failed";
const VISIBLE_RENDER_DEADLINE_MS = 12_000;

/** Presents one explicit Mermaid fence while the durable Chat content remains its source. */
export function MermaidDiagram({ source }: { source: string }) {
  const rootRef = useRef<HTMLElement>(null);
  const [nearViewport, setNearViewport] = useState(() => typeof IntersectionObserver === "undefined");
  const [result, setResult] = useState<MermaidDiagramResult>();
  const [mode, setMode] = useState<"diagram" | "source">("diagram");
  const [expanded, setExpanded] = useState(false);
  const [attempt, setAttempt] = useState(1);
  const theme = useMermaidTheme();
  const descriptionId = useId();
  const sourceId = useId();

  useEffect(() => {
    const root = rootRef.current;
    if (!root || typeof IntersectionObserver === "undefined") return undefined;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) {
        setNearViewport(true);
        observer.disconnect();
      }
    }, { rootMargin: "400px 0px" });
    observer.observe(root);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!nearViewport) return undefined;
    let current = true;
    setResult(undefined);
    // Queueing and renderer startup happen below the component boundary. Keep
    // the visible state bounded even if either layer stops making progress.
    const deadline = globalThis.setTimeout(() => {
      if (!current) return;
      current = false;
      setResult({ kind: "startup_timeout" });
    }, VISIBLE_RENDER_DEADLINE_MS);
    void renderMermaidDiagram(source, theme, attempt).then((next) => {
      if (!current) return;
      globalThis.clearTimeout(deadline);
      setResult(next);
    });
    return () => {
      current = false;
      globalThis.clearTimeout(deadline);
    };
  }, [attempt, nearViewport, source, theme]);

  const success = result?.kind === "success" ? result : undefined;
  const showSource = mode === "source" || !success;
  const status = result && result.kind !== "success"
    ? fallbackPresentation(result.kind)
    : nearViewport && !result
      ? { loading: true, message: "Rendering diagram…", retryable: false }
      : undefined;
  const label = success?.title || "Mermaid diagram";

  return (
    <figure className="agent-mermaid" data-mode={showSource ? "source" : "diagram"} ref={rootRef}>
      {showSource ? (
        <MermaidSource
          status={status}
          onRetry={status?.retryable ? () => setAttempt((value) => value + 1) : undefined}
          onShowDiagram={success ? () => setMode("diagram") : undefined}
          source={source}
        />
      ) : (
        <div className="agent-mermaid-viewport">
          <ImagePreviewViewport
            contentNoun="diagram"
            image={{ label, url: success.url }}
            imageClassName="agent-mermaid-image"
            toolbarActions={(
              <>
                <MermaidCopyButton source={source} />
                <button
                  aria-label="View diagram source"
                  className="attachment-preview-action"
                  onClick={() => setMode("source")}
                  type="button"
                >
                  <Code2 aria-hidden="true" size={16} />
                  <span className="visually-hidden">View source</span>
                </button>
                <button
                  aria-label="Expand diagram"
                  className="attachment-preview-action"
                  onClick={() => setExpanded(true)}
                  type="button"
                >
                  <Maximize2 aria-hidden="true" size={16} />
                  <span className="visually-hidden">Expand diagram</span>
                </button>
              </>
            )}
          />
          <span className="visually-hidden" id={descriptionId}>
            {success.description || "The diagram has no authored description. Its Mermaid source follows."}
          </span>
          <pre className="visually-hidden" id={sourceId}>{source}</pre>
        </div>
      )}
      {expanded && success ? (
        <AttachmentImagePreviewLightbox
          contentNoun="diagram"
          image={{ label, url: success.url }}
          onClose={() => setExpanded(false)}
          toolbarActions={(
            <button
              aria-label="View diagram source"
              className="attachment-preview-action"
              onClick={() => {
                setExpanded(false);
                setMode("source");
              }}
              type="button"
            >
              <Code2 aria-hidden="true" size={16} />
            </button>
          )}
        />
      ) : null}
    </figure>
  );
}

function MermaidSource({
  status,
  onRetry,
  onShowDiagram,
  source,
}: {
  status?: { loading?: boolean; message: string; retryable: boolean };
  onRetry?: () => void;
  onShowDiagram?: () => void;
  source: string;
}) {
  return (
    <div className="agent-mermaid-source">
      {status ? (
        <div className="agent-mermaid-status" role="status">
          <span className="agent-mermaid-status-label">
            {status.loading ? <LoaderCircle aria-hidden="true" className="agent-mermaid-spinner" size={14} /> : null}
            <span>{status.message}</span>
          </span>
          {onRetry ? (
            <button className="agent-mermaid-action" onClick={onRetry} type="button">
              <RotateCcw aria-hidden="true" size={13} />
              <span>Retry</span>
            </button>
          ) : null}
        </div>
      ) : null}
      <div className="agent-mermaid-source-actions">
        <MermaidCopyButton source={source} />
        {onShowDiagram ? (
          <button className="agent-mermaid-action" onClick={onShowDiagram} type="button">
            <ImageIcon aria-hidden="true" size={13} />
            <span>Show diagram</span>
          </button>
        ) : null}
      </div>
      <pre><code className="language-mermaid">{source}</code></pre>
    </div>
  );
}

function MermaidCopyButton({ source }: { source: string }) {
  const [state, setState] = useState<CopyState>("idle");
  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timerRef.current), []);
  const text = state === "copied" ? "Copied" : state === "failed" ? "Copy failed" : "Copy source";
  return (
    <button
      aria-label={text}
      className="attachment-preview-action"
      onClick={async () => {
        clearTimeout(timerRef.current);
        try {
          await copyText(source);
          setState("copied");
        } catch {
          setState("failed");
        }
        timerRef.current = setTimeout(() => setState("idle"), 1_400);
      }}
      type="button"
    >
      <Copy aria-hidden="true" size={16} />
      <span className="visually-hidden">{text}</span>
    </button>
  );
}

function fallbackPresentation(kind: Exclude<MermaidDiagramResult["kind"], "success">) {
  if (kind === "invalid" || kind === "failed") return { message: "Diagram could not be rendered", retryable: false };
  if (kind === "too_large") return { message: "Diagram is too large to render", retryable: false };
  if (kind === "timeout") return { message: "Diagram rendering timed out", retryable: true };
  if (kind === "startup_timeout") return { message: "Diagram renderer took too long to start", retryable: true };
  return { message: "Diagram renderer unavailable", retryable: true };
}
