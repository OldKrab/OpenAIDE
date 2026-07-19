import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(packageRoot, "../..");
const debugBuild = process.env.OPENAIDE_WEB_DEBUG_BUILD === "1";

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
  // Local Driver/Target bundles favor profiler readability without enabling React's dev runtime.
  esbuild: debugBuild ? { keepNames: true } : undefined,
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
    minify: !debugBuild,
    sourcemap: true,
    rolldownOptions: {
      output: {
        entryFileNames: "assets/[name].js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: "assets/[name][extname]",
        // Preserve stable vendor boundaries with Rolldown's supported code-splitting API.
        codeSplitting: {
          groups: [
            { name: "icons", test: /node_modules[\\/]lucide-react[\\/]/ },
            { name: "markdown", test: /node_modules[\\/](?:react-markdown|remark-gfm)[\\/]/ },
            { name: "react", test: /node_modules[\\/](?:react|react-dom)[\\/]/ },
            { name: "search", test: /node_modules[\\/]fuzzysort[\\/]/ },
          ],
        },
      }
    }
  }
});
