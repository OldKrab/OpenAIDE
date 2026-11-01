// The sandbox has no network, so the JPEG 2000 and JBIG2 decoders and the CMaps for
// CJK fonts travel inside this bundle as base64 and reach the worker on demand.
const wasmFiles = {
  ...import.meta.glob<string>("../../../../node_modules/pdfjs-dist/wasm/openjpeg.wasm", { eager: true, import: "default", query: "?inline" }),
  ...import.meta.glob<string>("../../../../node_modules/pdfjs-dist/wasm/jbig2.wasm", { eager: true, import: "default", query: "?inline" }),
};
const cMapFiles = import.meta.glob<string>("../../../../node_modules/pdfjs-dist/cmaps/*.bcmap", { eager: true, import: "default", query: "?inline" });

const byFilename = (files: Record<string, string>) =>
  new Map(Object.entries(files).map(([path, dataUrl]) => [path.slice(path.lastIndexOf("/") + 1), dataUrl]));

const wasm = byFilename(wasmFiles);
const cMaps = byFilename(cMapFiles);

/** Any trailing-slash value: pdf.js only hands it back through the factory below. */
export const BUNDLED_URL = "bundled/";

/**
 * pdf.js's seam for binary data when `useWorkerFetch` is off. A file not in the bundle
 * rejects, and pdf.js then degrades that one image or font rather than the document.
 */
export class BundledBinaryDataFactory {
  async fetch({ filename, kind }: { filename: string; kind: string }): Promise<Uint8Array> {
    const dataUrl = (kind === "wasmUrl" ? wasm : kind === "cMapUrl" ? cMaps : undefined)?.get(filename);
    if (!dataUrl) throw new Error(`pdf_binary_data_missing kind=${kind}`);
    return Uint8Array.from(atob(dataUrl.slice(dataUrl.indexOf(",") + 1)), (character) => character.charCodeAt(0));
  }
}
