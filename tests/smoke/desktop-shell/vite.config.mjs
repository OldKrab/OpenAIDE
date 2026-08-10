import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import path from "node:path";
import { fileURLToPath } from "node:url";

const testRoot = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(testRoot, "../../..");

export default defineConfig({
  root: testRoot,
  plugins: [react()],
  resolve: {
    alias: {
      "@openaide/app-server-client": path.resolve(repoRoot, "packages/app-server-client/src/index.ts"),
      "@openaide/app-shell-contracts": path.resolve(repoRoot, "packages/app-shell-contracts/src/index.ts"),
    },
  },
  build: {
    sourcemap: true,
  },
});
