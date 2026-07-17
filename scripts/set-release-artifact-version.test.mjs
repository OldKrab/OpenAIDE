import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { setReleaseArtifactVersion } from "./set-release-artifact-version.mjs";

test("stamps the exact release version into every packaged artifact manifest", () => {
  const repoRoot = fixtureRepo();

  setReleaseArtifactVersion(repoRoot, "0.0.1-alpha.9");

  assert.equal(packageJson(repoRoot, "apps/vscode-extension/package.json").version, "0.0.1-alpha.9");
  for (const relativePath of [
    "openaide-rs/app-server/Cargo.toml",
    "openaide-rs/app-server-protocol/Cargo.toml",
  ]) {
    assert.match(readFileSync(path.join(repoRoot, relativePath), "utf8"), /^version = "0\.0\.1-alpha\.9"$/m);
  }
});

test("rejects invalid versions and non-neutral source manifests", () => {
  assert.throws(() => setReleaseArtifactVersion(fixtureRepo(), "v0.0.1"), /must be SemVer/);

  const repoRoot = fixtureRepo();
  const extensionPath = path.join(repoRoot, "apps/vscode-extension/package.json");
  writeFileSync(extensionPath, `${JSON.stringify({ name: "test-extension", private: true, version: "0.0.1" })}\n`);
  assert.throws(() => setReleaseArtifactVersion(repoRoot, "0.0.2"), /must use neutral source version/);
});

function fixtureRepo() {
  const repoRoot = mkdtempSync(path.join(os.tmpdir(), "openaide-release-version-"));
  writeFixture(repoRoot, "apps/vscode-extension/package.json", {
    name: "test-extension",
    private: true,
    version: "0.0.0",
  });
  writeFixture(repoRoot, "openaide-rs/app-server/Cargo.toml", `[package]\nname = "app-server"\nversion = "0.0.0"\n`);
  writeFixture(repoRoot, "openaide-rs/app-server-protocol/Cargo.toml", `[package]\nname = "protocol"\nversion = "0.0.0"\n`);
  return repoRoot;
}

function writeFixture(repoRoot, relativePath, content) {
  const filePath = path.join(repoRoot, relativePath);
  mkdirSync(path.dirname(filePath), { recursive: true });
  const serialized = typeof content === "string" ? content : `${JSON.stringify(content, null, 2)}\n`;
  writeFileSync(filePath, serialized);
}

function packageJson(repoRoot, relativePath) {
  return JSON.parse(readFileSync(path.join(repoRoot, relativePath), "utf8"));
}
