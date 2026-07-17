import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SEMVER_PATTERN = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z]+(\.[0-9A-Za-z]+)*)?$/;
const NEUTRAL_VERSION = "0.0.0";

/**
 * Stamps the canonical release version into manifests consumed by packaged artifacts.
 * Source manifests stay neutral so package managers never become competing version owners.
 */
export function setReleaseArtifactVersion(repoRoot, version) {
  if (!SEMVER_PATTERN.test(version)) {
    throw new Error(`Release version must be SemVer without a v prefix: ${version}`);
  }

  setJsonVersion(path.join(repoRoot, "apps/vscode-extension/package.json"), version);
  for (const relativePath of [
    "openaide-rs/app-server/Cargo.toml",
    "openaide-rs/app-server-protocol/Cargo.toml",
  ]) {
    setCargoVersion(path.join(repoRoot, relativePath), version);
  }
}

function setJsonVersion(filePath, version) {
  const manifest = JSON.parse(readFileSync(filePath, "utf8"));
  if (manifest.version !== NEUTRAL_VERSION) {
    throw new Error(`${filePath} must use neutral source version ${NEUTRAL_VERSION}`);
  }
  manifest.version = version;
  writeFileSync(filePath, `${JSON.stringify(manifest, null, 2)}\n`);
}

function setCargoVersion(filePath, version) {
  const manifest = readFileSync(filePath, "utf8");
  const neutralVersionLine = `version = "${NEUTRAL_VERSION}"`;
  if (!manifest.startsWith(`[package]\n`) || !manifest.includes(`${neutralVersionLine}\n`)) {
    throw new Error(`${filePath} must use neutral package version ${NEUTRAL_VERSION}`);
  }
  writeFileSync(filePath, manifest.replace(neutralVersionLine, `version = "${version}"`));
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : undefined;
if (invokedPath === fileURLToPath(import.meta.url)) {
  const version = process.argv[2];
  if (!version) {
    throw new Error("Usage: node scripts/set-release-artifact-version.mjs <version>");
  }
  const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  setReleaseArtifactVersion(repoRoot, version);
  console.log(`Stamped release artifact manifests with version ${version}.`);
}
