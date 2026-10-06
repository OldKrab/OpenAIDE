export const PDF_RENDERER_VERSION = "6.4.299";
/** The whole document is buffered by the parent and the renderer; larger files keep Download only. */
export const PDF_SOURCE_LIMIT_BYTES = 64 * 1024 * 1024;

export type PdfViewerTheme = {
  background: string;
  text: string;
};

export type PdfZoom = "in" | "out" | "fit";
export type PdfLoadFailure = "invalid" | "encrypted" | "failed";

export type PdfViewerRequest =
  | { type: "openaide.pdf.load"; bytes: ArrayBuffer; theme: PdfViewerTheme }
  | { type: "openaide.pdf.theme"; theme: PdfViewerTheme }
  | { type: "openaide.pdf.zoom"; zoom: PdfZoom };

export type PdfViewerEvent =
  | { type: "openaide.pdf.ready"; version: string }
  | { type: "openaide.pdf.loaded"; pageCount: number }
  | { type: "openaide.pdf.failed"; reason: PdfLoadFailure }
  | { type: "openaide.pdf.page"; page: number }
  | { type: "openaide.pdf.scale"; fit: boolean; percent: number };

const MAX_PAGES = 1_000_000;

export function isPdfViewerRequest(value: unknown): value is PdfViewerRequest {
  if (!isRecord(value)) return false;
  switch (value.type) {
    case "openaide.pdf.load":
      return value.bytes instanceof ArrayBuffer
        && value.bytes.byteLength <= PDF_SOURCE_LIMIT_BYTES
        && isPdfViewerTheme(value.theme);
    case "openaide.pdf.theme":
      return isPdfViewerTheme(value.theme);
    case "openaide.pdf.zoom":
      return value.zoom === "in" || value.zoom === "out" || value.zoom === "fit";
    default:
      return false;
  }
}

/** The renderer parses untrusted documents, so the parent accepts only bounded scalar events. */
export function isPdfViewerEvent(value: unknown): value is PdfViewerEvent {
  if (!isRecord(value)) return false;
  switch (value.type) {
    case "openaide.pdf.ready":
      return value.version === PDF_RENDERER_VERSION;
    case "openaide.pdf.loaded":
      return isCount(value.pageCount);
    case "openaide.pdf.failed":
      return value.reason === "invalid" || value.reason === "encrypted" || value.reason === "failed";
    case "openaide.pdf.page":
      return isCount(value.page);
    case "openaide.pdf.scale":
      return typeof value.fit === "boolean"
        && typeof value.percent === "number"
        && Number.isFinite(value.percent)
        && value.percent > 0
        && value.percent <= 10_000;
    default:
      return false;
  }
}

function isCount(value: unknown) {
  return typeof value === "number" && Number.isInteger(value) && value >= 1 && value <= MAX_PAGES;
}

function isPdfViewerTheme(value: unknown): value is PdfViewerTheme {
  return isRecord(value)
    && ["background", "text"].every((key) => typeof value[key] === "string" && (value[key] as string).length <= 200);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
