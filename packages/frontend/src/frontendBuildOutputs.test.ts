import { execFile } from "node:child_process";
import { access, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { test } from "vitest";

const execFileAsync = promisify(execFile);
const packageRoot = fileURLToPath(new URL("..", import.meta.url));

test("a custom frontend output directory receives the app and Diagram renderer", async () => {
  const outDir = await mkdtemp(path.join(tmpdir(), "openaide-frontend-build-"));
  try {
    await execFileAsync("npm", ["run", "build", "--", "--outDir", outDir, "--emptyOutDir"], {
      cwd: packageRoot,
      maxBuffer: 10 * 1024 * 1024,
    });

    await Promise.all([
      access(path.join(outDir, "index.html")),
      access(path.join(outDir, "assets", "index.js")),
      access(path.join(outDir, "mermaid-renderer.html")),
      access(path.join(outDir, "mermaid-renderer.js")),
    ]);
  } finally {
    await rm(outDir, { recursive: true, force: true });
  }
}, 30_000);
