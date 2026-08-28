import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SEMVER_PATTERN = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z]+(\.[0-9A-Za-z]+)*)?$/;
const NEUTRAL_VERSION = "0.0.0";
const ROOT_CARGO_PACKAGE_NAMES = ["openaide-app-server", "openaide-app-server-protocol"];

/**
 * Stamps the canonical release version into manifests consumed by packaged artifacts.
 * Source manifests stay neutral so package managers never become competing version owners.
 */
export function setReleaseArtifactVersion(repoRoot, version) {
  if (!SEMVER_PATTERN.test(version)) {
    throw new Error(`Release version must be SemVer without a v prefix: ${version}`);
  }

  setJsonVersion(path.join(repoRoot, "apps/vscode-extension/package.json"), version);
  setJsonVersion(path.join(repoRoot, "apps/desktop/package.json"), version);
  setJsonVersion(path.join(repoRoot, "apps/desktop/src-tauri/tauri.conf.json"), version);
  for (const relativePath of [
    "openaide-rs/app-server/Cargo.toml",
    "openaide-rs/app-server-protocol/Cargo.toml",
    "apps/desktop/src-tauri/Cargo.toml",
  ]) {
    setCargoVersion(path.join(repoRoot, relativePath), version);
  }
  // Keep the lockfile in sync so release builds can retain Cargo's --locked guarantee.
  setCargoLockVersions(path.join(repoRoot, "Cargo.lock"), ROOT_CARGO_PACKAGE_NAMES, version);
  setCargoLockVersions(
    path.join(repoRoot, "apps/desktop/src-tauri/Cargo.lock"),
    ["openaide-desktop"],
    version,
  );
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
  const lineEnding = manifest.includes("\r\n") ? "\r\n" : "\n";
  const lines = manifest.split(/\r?\n/);
  const nextSectionIndex = lines.findIndex((line, index) => index > 0 && line.startsWith("["));
  const packageSectionEnd = nextSectionIndex === -1 ? lines.length : nextSectionIndex;
  const versionLineIndex = lines.findIndex(
    (line, index) => index > 0 && index < packageSectionEnd && line === neutralVersionLine,
  );

  if (lines[0] !== "[package]" || versionLineIndex === -1) {
    throw new Error(`${filePath} must use neutral package version ${NEUTRAL_VERSION}`);
  }

  // Git may check manifests out as CRLF on Windows, so preserve the checkout's line endings.
  lines[versionLineIndex] = `version = "${version}"`;
  writeFileSync(filePath, lines.join(lineEnding));
}

function setCargoLockVersions(filePath, packageNames, version) {
  const lockfile = readFileSync(filePath, "utf8");
  const lineEnding = lockfile.includes("\r\n") ? "\r\n" : "\n";
  const lines = lockfile.split(/\r?\n/);

  for (const packageName of packageNames) {
    const packageHeaderIndex = lines.findIndex(
      (line, index) => line === "[[package]]" && lines[index + 1] === `name = "${packageName}"`,
    );
    const nextPackageIndex = lines.findIndex(
      (line, index) => index > packageHeaderIndex && line === "[[package]]",
    );
    const packageEnd = nextPackageIndex === -1 ? lines.length : nextPackageIndex;
    const versionLineIndex = lines.findIndex(
      (line, index) => index > packageHeaderIndex && index < packageEnd && line === `version = "${NEUTRAL_VERSION}"`,
    );

    if (packageHeaderIndex === -1 || versionLineIndex === -1) {
      throw new Error(`${filePath} must contain a neutral package version for ${packageName}`);
    }

    lines[versionLineIndex] = `version = "${version}"`;
  }

  writeFileSync(filePath, lines.join(lineEnding));
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
