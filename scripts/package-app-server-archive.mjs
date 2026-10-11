import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SEMVER_PATTERN = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z]+(\.[0-9A-Za-z]+)*)?$/;

/** The one folder the archive unpacks to, so it never spills into the current directory. */
export const ARCHIVE_FOLDER = "openaide-app-server";
export const ARCHIVE_ENTRIES = ["LICENSE", "README.md", "VERSION", "openaide-app-server"];

export function appServerArchiveName(version) {
  return `openaide-app-server-linux-x64-${version}.tar.gz`;
}

/**
 * Packs the Linux App Server for a computer with no App Shell on it. The
 * archive is the executable alone with its guide: the App Server needs nothing
 * else installed to be paired with and reached.
 */
export function packageAppServerArchive({ binaryPath, version, outputDirectory }) {
  if (!SEMVER_PATTERN.test(version)) {
    throw new Error(`Release version must be SemVer without a v prefix: ${version}`);
  }
  const staging = mkdtempSync(path.join(os.tmpdir(), "openaide-app-server-archive-"));
  try {
    const folder = path.join(staging, ARCHIVE_FOLDER);
    mkdirSync(folder);
    copyFileSync(binaryPath, path.join(folder, "openaide-app-server"));
    // Artifact transfer between jobs drops the executable bit.
    chmodSync(path.join(folder, "openaide-app-server"), 0o755);
    copyFileSync(path.join(repoRoot, "deploy/app-server/README.md"), path.join(folder, "README.md"));
    copyFileSync(path.join(repoRoot, "LICENSE"), path.join(folder, "LICENSE"));
    writeFileSync(path.join(folder, "VERSION"), `${version}\n`);

    mkdirSync(outputDirectory, { recursive: true });
    const archivePath = path.join(outputDirectory, appServerArchiveName(version));
    // Fixed ownership keeps the builder's account out of a published archive.
    execFileSync("tar", [
      "--owner=0",
      "--group=0",
      "--numeric-owner",
      "-czf",
      archivePath,
      "-C",
      staging,
      ARCHIVE_FOLDER,
    ]);
    return archivePath;
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : undefined;
if (invokedPath === fileURLToPath(import.meta.url)) {
  const [binaryPath, version, outputDirectory] = process.argv.slice(2);
  if (!binaryPath || !version || !outputDirectory) {
    throw new Error(
      "Usage: node scripts/package-app-server-archive.mjs <binary-path> <version> <output-directory>",
    );
  }
  const archivePath = packageAppServerArchive({ binaryPath, version, outputDirectory });
  console.log(`Wrote ${path.basename(archivePath)}.`);
}
