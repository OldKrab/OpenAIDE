import { mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "vite";
import { expect, test } from "vitest";

const packageRoot = fileURLToPath(new URL("..", import.meta.url));
const configFile = path.join(packageRoot, "vite.config.ts");

test("local role bundle preserves function names for browser profiling", async () => {
  const outDir = await mkdtemp(path.join(tmpdir(), "openaide-profile-build-"));
  const previousDebugBuild = process.env.OPENAIDE_WEB_DEBUG_BUILD;
  process.env.OPENAIDE_WEB_DEBUG_BUILD = "1";

  try {
    await build({ root: packageRoot, configFile, build: { outDir, emptyOutDir: true } });
    const assetNames = await readdir(path.join(outDir, "assets"));
    const entryName = assetNames.find((name) => name === "index.js");
    expect(entryName).toBe("index.js");

    const entry = await readFile(path.join(outDir, "assets", entryName!), "utf8");
    expect(entry).toContain("useLiveMessagePresentation");
  } finally {
    if (previousDebugBuild === undefined) delete process.env.OPENAIDE_WEB_DEBUG_BUILD;
    else process.env.OPENAIDE_WEB_DEBUG_BUILD = previousDebugBuild;
    await rm(outDir, { recursive: true, force: true });
  }
}, 20_000);
