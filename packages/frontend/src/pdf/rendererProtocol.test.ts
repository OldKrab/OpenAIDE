import { describe, expect, it } from "vitest";
import {
  PDF_RENDERER_VERSION,
  PDF_SOURCE_LIMIT_BYTES,
  isPdfViewerEvent,
  isPdfViewerRequest,
} from "./rendererProtocol";

const theme = { background: "rgb(10, 10, 10)", text: "rgb(230, 230, 230)" };

describe("PDF renderer protocol", () => {
  it("accepts only bounded scalar events from the renderer", () => {
    expect(isPdfViewerEvent({ type: "openaide.pdf.ready", version: PDF_RENDERER_VERSION })).toBe(true);
    expect(isPdfViewerEvent({ type: "openaide.pdf.ready", version: "0.0.0" })).toBe(false);
    expect(isPdfViewerEvent({ type: "openaide.pdf.loaded", pageCount: 12 })).toBe(true);
    expect(isPdfViewerEvent({ type: "openaide.pdf.loaded", pageCount: 0 })).toBe(false);
    expect(isPdfViewerEvent({ type: "openaide.pdf.loaded", pageCount: "12" })).toBe(false);
    expect(isPdfViewerEvent({ type: "openaide.pdf.page", page: 1.5 })).toBe(false);
    expect(isPdfViewerEvent({ type: "openaide.pdf.failed", reason: "encrypted" })).toBe(true);
    expect(isPdfViewerEvent({ type: "openaide.pdf.failed", reason: "<b>boom</b>" })).toBe(false);
    expect(isPdfViewerEvent({ type: "openaide.pdf.scale", fit: false, percent: 125 })).toBe(true);
    expect(isPdfViewerEvent({ type: "openaide.pdf.scale", fit: false, percent: Number.NaN })).toBe(false);
    expect(isPdfViewerEvent({ type: "openaide.mermaid.ready", version: PDF_RENDERER_VERSION })).toBe(false);
  });

  it("accepts only bounded requests in the renderer", () => {
    expect(isPdfViewerRequest({ type: "openaide.pdf.load", bytes: new ArrayBuffer(8), theme })).toBe(true);
    expect(isPdfViewerRequest({ type: "openaide.pdf.load", bytes: "%PDF-", theme })).toBe(false);
    expect(isPdfViewerRequest({
      type: "openaide.pdf.load",
      bytes: { byteLength: PDF_SOURCE_LIMIT_BYTES + 1 },
      theme,
    })).toBe(false);
    expect(isPdfViewerRequest({ type: "openaide.pdf.theme", theme: { ...theme, text: "x".repeat(201) } })).toBe(false);
    expect(isPdfViewerRequest({ type: "openaide.pdf.zoom", zoom: "in" })).toBe(true);
    expect(isPdfViewerRequest({ type: "openaide.pdf.zoom", zoom: "400%" })).toBe(false);
  });
});
