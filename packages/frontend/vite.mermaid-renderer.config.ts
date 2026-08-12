import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const packageRoot = path.dirname(fileURLToPath(import.meta.url));
const rendererDocument = readFileSync(path.resolve(packageRoot, "src/mermaid/renderer.html"), "utf8");

export default defineConfig({
  publicDir: false,
  plugins: [{
    name: "emit-mermaid-renderer-document",
    generateBundle() {
      this.emitFile({ type: "asset", fileName: "mermaid-renderer.html", source: rendererDocument });
    },
  }],
  build: {
    outDir: "dist",
    emptyOutDir: false,
    minify: true,
    sourcemap: true,
    lib: {
      entry: path.resolve(packageRoot, "src/mermaid/rendererEntry.ts"),
      formats: ["iife"],
      name: "OpenAIDEMermaidRenderer",
      fileName: () => "mermaid-renderer.js",
    },
    rolldownOptions: {
      output: {
        codeSplitting: false,
      },
    },
  },
});
