import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
import { fileURLToPath } from "node:url";

const harnessRoot = path.dirname(fileURLToPath(import.meta.url));
const frontendRoot = path.resolve(harnessRoot, "..");
const repoRoot = path.resolve(frontendRoot, "../..");
const prototypePort = Number(process.env.OPENAIDE_WEB_PROTOTYPE_PORT ?? "5572");

export default defineConfig({
  base: "/prototype/",
  root: harnessRoot,
  plugins: [react()],
  cacheDir: path.join(repoRoot, "node_modules/.vite-prototype"),
  resolve: {
    alias: {
      "@openaide/app-server-client": path.join(repoRoot, "packages/app-server-client/src/index.ts"),
      "@openaide/app-shell-contracts": path.join(repoRoot, "packages/app-shell-contracts/src/index.ts"),
    },
  },
  server: {
    host: "127.0.0.1",
    port: prototypePort,
    strictPort: true,
    fs: {
      // Prototypes may reuse any production module in this repository, but Vite remains loopback-only.
      allow: [repoRoot],
    },
  },
});
