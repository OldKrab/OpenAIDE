import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const packageRoot = path.dirname(fileURLToPath(import.meta.url));

// One self-contained script: the parent embeds it in a sandboxed document that cannot
// fetch chunks, stylesheets, or images of its own.
export default defineConfig({
  publicDir: false,
  build: {
    outDir: "dist",
    emptyOutDir: false,
    minify: true,
    // The bundle is almost entirely pdf.js and its worker; their maps would outweigh the script.
    sourcemap: false,
    assetsInlineLimit: Number.MAX_SAFE_INTEGER,
    lib: {
      entry: path.resolve(packageRoot, "src/pdf/rendererEntry.ts"),
      formats: ["iife"],
      name: "OpenAIDEPdfRenderer",
      fileName: () => "pdf-renderer.js",
    },
    rolldownOptions: {
      output: {
        codeSplitting: false,
      },
    },
  },
});
