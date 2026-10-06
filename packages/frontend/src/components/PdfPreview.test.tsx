// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FileViewerSnapshot } from "@openaide/app-server-client";
import { PDF_RENDERER_VERSION, PDF_SOURCE_LIMIT_BYTES } from "../pdf/rendererProtocol";
import type { FileViewerContent, FileViewerContentResult } from "../services/frontendShell";
import { PdfPreview } from "./PdfPreview";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const tab = {
  handle: "handle-pdf",
  displayPath: "/work/report.pdf",
  basename: "report.pdf",
  kind: "pdf",
  truncated: false,
} as FileViewerSnapshot;

describe("PDF preview", () => {
  let surface: HTMLElement;
  let root: Root;
  const telemetry = vi.fn();
  beforeEach(() => {
    surface = document.createElement("div");
    document.body.append(surface);
    root = createRoot(surface);
    telemetry.mockClear();
    vi.stubGlobal("fetch", vi.fn(async () => new Response("/* renderer */")));
  });
  afterEach(async () => {
    await act(async () => root.unmount());
    surface.remove();
    vi.unstubAllGlobals();
  });

  const render = (content: FileViewerContent | undefined, onRefresh = vi.fn()) => act(async () => root.render(
    <PdfPreview content={content} onClose={vi.fn()} onRefresh={onRefresh} tab={tab} telemetry={telemetry} />,
  ));
  const reader = (result: FileViewerContentResult) => ({ read: vi.fn(async () => result) });
  const fromFrame = (frame: HTMLIFrameElement, data: unknown, source: unknown = frame.contentWindow) => act(async () => {
    window.dispatchEvent(new MessageEvent("message", { data, source: source as MessageEventSource }));
  });

  it("keeps the fallback when the App Shell cannot supply file bytes", async () => {
    await render(undefined);

    expect(surface.textContent).toContain("PDF preview unavailable");
    expect(surface.querySelector("iframe")).toBeNull();
    expect(surface.textContent).not.toContain("Retry");
  });

  it("explains a document above the preview budget and retries through Refresh", async () => {
    const content = reader({ kind: "tooLarge" });
    const onRefresh = vi.fn();
    await render(content, onRefresh);

    expect(content.read).toHaveBeenCalledWith(
      expect.objectContaining({ handle: "handle-pdf", maxBytes: PDF_SOURCE_LIMIT_BYTES }),
      expect.any(AbortSignal),
    );
    expect(surface.querySelector('[role="alert"]')?.textContent).toContain("PDF too large to preview");
    expect(surface.querySelector("iframe")).toBeNull();
    [...surface.querySelectorAll("button")].find((button) => button.textContent === "Retry")!.click();
    expect(onRefresh).toHaveBeenCalledWith("handle-pdf");
  });

  it("hands the bytes to a script-only sandbox and shows its page position", async () => {
    const bytes = new ArrayBuffer(16);
    await render(reader({ kind: "bytes", bytes }));
    const frame = surface.querySelector("iframe")!;
    expect(frame.getAttribute("sandbox")).toBe("allow-scripts");
    expect(frame.title).toBe("report.pdf preview");
    expect(frame.srcdoc).toContain("connect-src 'none'");
    const postMessage = vi.spyOn(frame.contentWindow!, "postMessage").mockImplementation(() => undefined);

    // Another window cannot drive the viewer, and nothing is sent before the renderer is ready.
    await fromFrame(frame, { type: "openaide.pdf.ready", version: PDF_RENDERER_VERSION }, window);
    expect(postMessage).not.toHaveBeenCalled();
    await fromFrame(frame, { type: "openaide.pdf.ready", version: PDF_RENDERER_VERSION });
    expect(postMessage).toHaveBeenCalledWith(expect.objectContaining({ type: "openaide.pdf.load", bytes }), "*", [bytes]);
    expect(surface.textContent).toContain("Opening PDF");

    await fromFrame(frame, { type: "openaide.pdf.loaded", pageCount: 3 });
    await fromFrame(frame, { type: "openaide.pdf.page", page: 2 });
    await fromFrame(frame, { type: "openaide.pdf.scale", fit: false, percent: 125 });
    expect(surface.textContent).toContain("Page 2 of 3");
    expect(telemetry.mock.calls.map(([event, fields]) => [event, fields.outcome, fields.page_count, fields.source_bytes])).toEqual([
      ["pdf_preview_start", undefined, undefined, undefined],
      ["pdf_preview_terminal", "success", 3, 16],
    ]);
    expect(surface.textContent).toContain("125%");

    surface.querySelector<HTMLButtonElement>('[aria-label="Fit PDF to width"]')!.click();
    expect(postMessage).toHaveBeenLastCalledWith({ type: "openaide.pdf.zoom", zoom: "fit" }, "*", []);
  });

  it("replaces the viewer with a classified failure for an encrypted document", async () => {
    await render(reader({ kind: "bytes", bytes: new ArrayBuffer(16) }));
    const frame = surface.querySelector("iframe")!;
    vi.spyOn(frame.contentWindow!, "postMessage").mockImplementation(() => undefined);

    await fromFrame(frame, { type: "openaide.pdf.ready", version: PDF_RENDERER_VERSION });
    await fromFrame(frame, { type: "openaide.pdf.failed", reason: "encrypted" });

    expect(surface.querySelector('[role="alert"]')?.textContent).toContain("Password-protected PDF");
    expect(telemetry).toHaveBeenLastCalledWith("pdf_preview_terminal", expect.objectContaining({ outcome: "encrypted" }));
    expect(surface.querySelector("iframe")).toBeNull();
  });
});
