import { buildSelfContainedRendererDocument } from "../mermaid/renderService";
import {
  isPdfViewerEvent,
  type PdfLoadFailure,
  type PdfViewerEvent,
  type PdfViewerRequest,
  type PdfViewerTheme,
  type PdfZoom,
} from "./rendererProtocol";

export type PdfFrameFailure = PdfLoadFailure | "renderer_unavailable" | "startup_timeout" | "load_timeout";

export type PdfViewerFrame = {
  /** Resolves once the document is open; later page and scale changes arrive through `onEvent`. */
  loaded: Promise<{ kind: "loaded"; pageCount: number } | { kind: "failed"; reason: PdfFrameFailure }>;
  setTheme(theme: PdfViewerTheme): void;
  zoom(zoom: PdfZoom): void;
  dispose(): void;
};

// The renderer is a large lazy-loaded bundle; parsing a 64 MiB document is slower still.
const RENDERER_READY_TIMEOUT_MS = 30_000;
const DOCUMENT_LOAD_TIMEOUT_MS = 60_000;

let rendererScript: Promise<string> | undefined;

/** Fetched once per page by the trusted parent: the opaque sandbox cannot authenticate. */
function loadRendererScript() {
  rendererScript ??= fetch(new URL("/pdf-renderer.js", window.location.href))
    .then((response) => {
      if (!response.ok) throw new Error("renderer unavailable");
      return response.text();
    })
    .catch((error: unknown) => {
      rendererScript = undefined;
      throw error;
    });
  return rendererScript;
}

export function buildPdfViewerDocument(script: string, nonce: string) {
  // TODO: move the self-contained sandbox document builder out of the Mermaid module;
  // it is shared by both renderers and only its payload element id is Mermaid-named.
  return buildSelfContainedRendererDocument(`<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; connect-src 'none'; font-src data:; img-src data: blob:; style-src 'unsafe-inline'; worker-src blob:; script-src 'nonce-${nonce}';" />
    <title>OpenAIDE PDF Preview</title>
  </head>
  <body>
    <script nonce="${nonce}" src="./pdf-renderer.js"></script>
  </body>
</html>`, script, nonce);
}

/**
 * Mounts one visible, sandboxed viewer for one document. The frame has scripts but no
 * same-origin authority or network; the document bytes are transferred into it and only
 * validated scalar events come back.
 */
export function openPdfViewerFrame({
  bytes,
  host,
  onEvent,
  theme,
  title,
}: {
  bytes: ArrayBuffer;
  host: HTMLElement;
  onEvent: (event: Extract<PdfViewerEvent, { type: "openaide.pdf.page" | "openaide.pdf.scale" }>) => void;
  theme: PdfViewerTheme;
  title: string;
}): PdfViewerFrame {
  const frame = document.createElement("iframe");
  frame.className = "file-viewer-pdf-frame";
  frame.title = title;
  frame.setAttribute("sandbox", "allow-scripts");
  let disposed = false;
  let timeout: number | undefined;
  let settle: (result: Awaited<PdfViewerFrame["loaded"]>) => void = () => undefined;
  const loaded = new Promise<Awaited<PdfViewerFrame["loaded"]>>((resolve) => {
    settle = (result) => {
      window.clearTimeout(timeout);
      resolve(result);
    };
  });
  const send = (request: PdfViewerRequest, transfer: Transferable[] = []) => {
    frame.contentWindow?.postMessage(request, "*", transfer);
  };
  const onMessage = (event: MessageEvent) => {
    if (disposed || event.source !== frame.contentWindow || !isPdfViewerEvent(event.data)) return;
    const message = event.data;
    switch (message.type) {
      case "openaide.pdf.ready":
        window.clearTimeout(timeout);
        timeout = window.setTimeout(() => settle({ kind: "failed", reason: "load_timeout" }), DOCUMENT_LOAD_TIMEOUT_MS);
        send({ type: "openaide.pdf.load", bytes, theme }, [bytes]);
        break;
      case "openaide.pdf.loaded":
        settle({ kind: "loaded", pageCount: message.pageCount });
        break;
      case "openaide.pdf.failed":
        settle({ kind: "failed", reason: message.reason });
        break;
      default:
        onEvent(message);
    }
  };
  window.addEventListener("message", onMessage);
  timeout = window.setTimeout(() => settle({ kind: "failed", reason: "startup_timeout" }), RENDERER_READY_TIMEOUT_MS);
  host.append(frame);
  void loadRendererScript().then(
    (script) => {
      if (!disposed) frame.srcdoc = buildPdfViewerDocument(script, globalThis.crypto.randomUUID().replaceAll("-", ""));
    },
    () => settle({ kind: "failed", reason: "renderer_unavailable" }),
  );
  return {
    loaded,
    setTheme: (next) => send({ type: "openaide.pdf.theme", theme: next }),
    zoom: (zoom) => send({ type: "openaide.pdf.zoom", zoom }),
    dispose() {
      disposed = true;
      window.clearTimeout(timeout);
      window.removeEventListener("message", onMessage);
      frame.remove();
    },
  };
}
