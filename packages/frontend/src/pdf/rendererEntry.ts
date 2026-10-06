// The legacy build carries its own polyfills: the default one requires JavaScript built-ins
// newer than the browsers and webviews OpenAIDE supports, and fails before first paint there.
import { AnnotationMode, GlobalWorkerOptions, getDocument, type PDFDocumentLoadingTask } from "pdfjs-dist/legacy/build/pdf.mjs";
import { EventBus, PDFLinkService, PDFViewer } from "pdfjs-dist/legacy/web/pdf_viewer.mjs";
import viewerStyles from "pdfjs-dist/legacy/web/pdf_viewer.css?inline";
// A classic inline worker: the opaque-origin sandbox refuses module workers from blob URLs.
import PdfWorker from "pdfjs-dist/legacy/build/pdf.worker.mjs?worker&inline";
import {
  PDF_RENDERER_VERSION,
  isPdfViewerRequest,
  type PdfLoadFailure,
  type PdfViewerEvent,
  type PdfViewerTheme,
  type PdfZoom,
} from "./rendererProtocol";

// This document is the whole sandbox: it has an opaque origin and no network, so the
// viewer stylesheet and the parsing worker travel inside this bundle.
const FIT = "page-width";

const style = document.createElement("style");
style.textContent = `${viewerStyles}
  html, body { height: 100%; margin: 0; }
  body { overflow: hidden; font-family: system-ui, sans-serif; }
  #container { position: absolute; inset: 0; overflow: auto; }
  .pdfViewer .page { border: 0; box-shadow: 0 1px 4px rgb(0 0 0 / 0.28); }
`;
document.head.append(style);

const container = document.createElement("div");
container.id = "container";
container.tabIndex = 0;
const pages = document.createElement("div");
pages.className = "pdfViewer";
container.append(pages);
document.body.append(container);

const eventBus = new EventBus();
const linkService = new PDFLinkService({ eventBus });
// In-document destinations still navigate; the sandbox has nowhere to open external targets.
linkService.externalLinkEnabled = false;
const viewer = new PDFViewer({
  container,
  viewer: pages,
  eventBus,
  linkService,
  // Read-only preview: annotations draw, form widgets and editors stay inert.
  annotationMode: AnnotationMode.ENABLE,
  enableAutoLinking: false,
});
linkService.setViewer(viewer);

let loadingTask: PDFDocumentLoadingTask | undefined;

eventBus.on("pagesinit", () => {
  viewer.currentScaleValue = FIT;
});
eventBus.on("pagechanging", (event: { pageNumber: number }) => {
  post({ type: "openaide.pdf.page", page: event.pageNumber });
});
eventBus.on("scalechanging", (event: { scale: number; presetValue?: string }) => {
  post({ type: "openaide.pdf.scale", fit: event.presetValue === FIT, percent: Math.round(event.scale * 100) });
});
new ResizeObserver(() => {
  if (!viewer.pdfDocument) return;
  // A fitted document follows the panel width; an explicit zoom level is kept.
  if (viewer.currentScaleValue === FIT) viewer.currentScaleValue = FIT;
  viewer.update();
}).observe(container);

window.addEventListener("message", (event) => {
  if (event.source !== window.parent || !isPdfViewerRequest(event.data)) return;
  const request = event.data;
  if (request.type === "openaide.pdf.theme") applyTheme(request.theme);
  else if (request.type === "openaide.pdf.zoom") zoom(request.zoom);
  else {
    applyTheme(request.theme);
    void load(request.bytes);
  }
});

post({ type: "openaide.pdf.ready", version: PDF_RENDERER_VERSION });

/** One frame shows one document; the parent replaces the frame to show another snapshot. */
async function load(bytes: ArrayBuffer) {
  if (loadingTask) return;
  try {
    const worker = new PdfWorker();
    // A worker that cannot start never answers; fail now instead of at the parent's timeout.
    worker.addEventListener("error", () => {
      if (!viewer.pdfDocument) void loadingTask?.destroy();
    });
    GlobalWorkerOptions.workerPort = worker;
    const task = getDocument({
      data: new Uint8Array(bytes),
      // No scripted forms or fetched decoder assets inside the sandbox.
      // TODO: package the pdf.js wasm decoders and CMaps into this bundle; without them
      // JPEG 2000 images and some CJK fonts degrade instead of rendering exactly.
      enableXfa: false,
      useWasm: false,
      useWorkerFetch: false,
      useSystemFonts: true,
    });
    loadingTask = task;
    let encrypted = false;
    task.onPassword = () => {
      encrypted = true;
      void task.destroy();
    };
    const pdfDocument = await task.promise.catch((error: unknown) => {
      throw encrypted ? new EncryptedDocumentError() : error;
    });
    viewer.setDocument(pdfDocument);
    linkService.setDocument(pdfDocument);
    post({ type: "openaide.pdf.loaded", pageCount: pdfDocument.numPages });
  } catch (error) {
    post({ type: "openaide.pdf.failed", reason: classifiedFailure(error) });
  }
}

function zoom(direction: PdfZoom) {
  if (!viewer.pdfDocument) return;
  if (direction === "fit") viewer.currentScaleValue = FIT;
  else viewer.updateScale({ steps: direction === "in" ? 1 : -1 });
}

function applyTheme(theme: PdfViewerTheme) {
  // Property assignment cannot introduce new declarations; invalid colors are ignored.
  document.body.style.backgroundColor = theme.background;
  document.body.style.color = theme.text;
  document.documentElement.style.colorScheme = isDark(document.body) ? "dark" : "light";
}

function isDark(element: HTMLElement) {
  const channels = getComputedStyle(element).backgroundColor.match(/[\d.]+/g)?.slice(0, 3).map(Number);
  if (!channels || channels.length < 3) return false;
  const [red, green, blue] = channels;
  return (red * 299 + green * 587 + blue * 114) / 1000 < 128;
}

class EncryptedDocumentError extends Error {}

function classifiedFailure(error: unknown): PdfLoadFailure {
  if (error instanceof EncryptedDocumentError) return "encrypted";
  const name = error instanceof Error ? error.name : "";
  if (name === "PasswordException") return "encrypted";
  if (name === "InvalidPDFException" || name === "FormatError") return "invalid";
  return "failed";
}

function post(event: PdfViewerEvent) {
  window.parent.postMessage(event, "*");
}
