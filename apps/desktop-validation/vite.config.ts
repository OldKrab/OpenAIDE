import react from "@vitejs/plugin-react";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const packageRoot = path.dirname(fileURLToPath(import.meta.url));
const repositoryRoot = path.resolve(packageRoot, "../..");

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@openaide/app-server-client": path.resolve(repositoryRoot, "packages/app-server-client/src/index.ts"),
      "@openaide/app-shell-contracts": path.resolve(repositoryRoot, "packages/app-shell-contracts/src/index.ts"),
    },
  },
  server: {
    host: "127.0.0.1",
    port: 14320,
    strictPort: true,
    fs: { allow: [repositoryRoot, packageRoot] },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: true,
  },
});
