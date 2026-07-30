import react from "@vitejs/plugin-react";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const prototypeRoot = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(prototypeRoot, "../../..");

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@openaide/app-server-client": path.resolve(
        repoRoot,
        "packages/app-server-client/src/index.ts",
      ),
      "@openaide/app-shell-contracts": path.resolve(
        repoRoot,
        "packages/app-shell-contracts/src/index.ts",
      ),
    },
  },
  server: {
    host: "127.0.0.1",
    port: 14320,
    strictPort: true,
    fs: { allow: [repoRoot, prototypeRoot] },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
