import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { syncFrontendAssets } from "./sync-frontend-assets.mjs";

test("synced VS Code styles address bundled fonts relative to the stylesheet", () => {
  const root = mkdtempSync(join(tmpdir(), "openaide-frontend-assets-"));
  const source = join(root, "source");
  const target = join(root, "target");
  mkdirSync(join(source, "assets"), { recursive: true });
  writeFileSync(
    join(source, "assets", "index.css"),
    '@font-face{src:url(/assets/inter-latin.woff2)} body{background:url("/assets/noise.png")}',
  );

  syncFrontendAssets(source, target);

  assert.equal(
    readFileSync(join(target, "assets", "index.css"), "utf8"),
    '@font-face{src:url(./inter-latin.woff2)} body{background:url("./noise.png")}',
  );
});
