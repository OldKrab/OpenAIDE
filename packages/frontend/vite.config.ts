import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(packageRoot, "../..");

function allowedHostsFromEnv() {
  const rawHosts = process.env.OPENAIDE_VITE_ALLOWED_HOSTS;
  if (!rawHosts) return undefined;
  const hosts = rawHosts
    .split(",")
    .map((host) => host.trim())
    .filter(Boolean);
  return hosts.length > 0 ? hosts : undefined;
}

export default defineConfig({
  plugins: [react()],
  cacheDir: process.env.OPENAIDE_VITE_CACHE_DIR ?? "node_modules/.vite",
  resolve: {
    alias: {
      "@openaide/app-server-client": path.resolve(repoRoot, "packages/app-server-client/src/index.ts"),
      "@openaide/app-shell-contracts": path.resolve(repoRoot, "packages/app-shell-contracts/src/index.ts"),
    },
  },
  server: {
    allowedHosts: allowedHostsFromEnv()
  },
  build: {
    outDir: "dist",
    sourcemap: true,
    rollupOptions: {
      output: {
        entryFileNames: "assets/[name].js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: "assets/[name][extname]",
        manualChunks: {
          icons: ["lucide-react"],
          markdown: ["react-markdown", "remark-gfm"],
          react: ["react", "react-dom"],
          search: ["fuzzysort"],
        },
      }
    }
  }
});
