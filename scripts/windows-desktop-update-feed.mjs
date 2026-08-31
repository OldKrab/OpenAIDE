import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import semver from "semver";

const repository = "OldKrab/OpenAIDE";
const releasePrefix = `https://github.com/${repository}/releases/download/`;

/**
 * Builds a complete Windows feed from immutable GitHub releases.
 *
 * A release becomes an update-capable source only when it contains its own
 * updater bundle. Regenerating every source manifest makes Pages deployment
 * atomic and keeps dormant installations able to jump to the newest release.
 */
export function buildWindowsDesktopUpdateFeed({
  releases,
  targetVersion,
  artifactBytes,
  artifactSignature,
  notes,
}) {
  if (!Array.isArray(releases)) throw new Error("GitHub releases must be an array");
  releases = releases.flat();
  if (!semver.valid(targetVersion)) throw new Error("target version must be exact SemVer");
  if (!Buffer.isBuffer(artifactBytes) || artifactBytes.length === 0) {
    throw new Error("updater artifact is empty");
  }
  if (typeof artifactSignature !== "string" || artifactSignature.trim().length < 32) {
    throw new Error("updater signature is missing");
  }
  if (typeof notes !== "string" || Buffer.byteLength(notes) > 64 * 1024) {
    throw new Error("release notes must be bounded text");
  }

  const targetTag = `v${targetVersion}`;
  const targetRelease = releases.find(
    (release) => release.tag_name === targetTag && !release.draft,
  );
  if (!targetRelease) throw new Error(`published release ${targetTag} was not found`);
  const targetAssetName = installerAssetName(targetVersion);
  const targetAsset = targetRelease.assets?.find((asset) => asset.name === targetAssetName);
  if (!targetAsset) throw new Error(`${targetTag} does not contain ${targetAssetName}`);
  if (
    typeof targetAsset.browser_download_url !== "string"
    || !targetAsset.browser_download_url.startsWith(`${releasePrefix}${targetTag}/`)
  ) {
    throw new Error("target updater URL is not an immutable canonical release URL");
  }
  if (targetAsset.size !== undefined && targetAsset.size !== artifactBytes.length) {
    throw new Error("local updater artifact does not match the published asset size");
  }

  const targetIsPrerelease = Boolean(targetRelease.prerelease);
  const channel = targetIsPrerelease ? "prerelease" : "stable";
  const digest = crypto.createHash("sha256").update(artifactBytes).digest("hex");
  const manifest = {
    version: targetVersion,
    notes,
    ...(targetRelease.published_at ? { pub_date: targetRelease.published_at } : {}),
    url: targetAsset.browser_download_url,
    signature: artifactSignature.trim(),
    signingIdentity: "release",
    artifactSize: artifactBytes.length,
    artifactSha256: digest,
  };

  const feed = new Map();
  for (const release of releases) {
    const source = releaseVersion(release);
    if (
      !source
      || release.draft
      || Boolean(release.prerelease) !== targetIsPrerelease
      || semver.gt(source, targetVersion)
      || !hasUpdaterArtifacts(release, source)
    ) {
      continue;
    }
    feed.set(`v1/${channel}/windows/x86_64/${source}.json`, manifest);
  }
  if (!feed.has(`v1/${channel}/windows/x86_64/${targetVersion}.json`)) {
    throw new Error("target release is not an update-capable source");
  }
  return feed;
}

export function writeWindowsDesktopUpdateFeed(feed, outputDirectory) {
  fs.rmSync(outputDirectory, { force: true, recursive: true });
  for (const [relativePath, manifest] of feed) {
    const destination = path.join(outputDirectory, relativePath);
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.writeFileSync(destination, `${JSON.stringify(manifest, null, 2)}\n`);
  }
}

function releaseVersion(release) {
  const tag = typeof release.tag_name === "string" ? release.tag_name : "";
  const version = tag.startsWith("v") ? tag.slice(1) : "";
  return semver.valid(version) ? version : undefined;
}

function installerAssetName(version) {
  return `openaide-desktop-windows-x64-${version}.exe`;
}

function hasUpdaterArtifacts(release, version) {
  const installer = installerAssetName(version);
  const names = new Set(release.assets?.map((asset) => asset.name));
  return names.has(installer) && names.has(`${installer}.sig`);
}

function parseArguments(arguments_) {
  const values = new Map();
  for (let index = 0; index < arguments_.length; index += 2) {
    const key = arguments_[index];
    const value = arguments_[index + 1];
    if (!key?.startsWith("--") || value === undefined) throw new Error("invalid arguments");
    values.set(key.slice(2), value);
  }
  for (const required of ["releases", "version", "artifact", "signature", "notes", "output"]) {
    if (!values.has(required)) throw new Error(`--${required} is required`);
  }
  return values;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    const arguments_ = parseArguments(process.argv.slice(2));
    const feed = buildWindowsDesktopUpdateFeed({
      releases: JSON.parse(fs.readFileSync(arguments_.get("releases"), "utf8")),
      targetVersion: arguments_.get("version"),
      artifactBytes: fs.readFileSync(arguments_.get("artifact")),
      artifactSignature: fs.readFileSync(arguments_.get("signature"), "utf8"),
      notes: fs.readFileSync(arguments_.get("notes"), "utf8"),
    });
    writeWindowsDesktopUpdateFeed(feed, arguments_.get("output"));
    process.stdout.write(`Generated ${feed.size} Windows Desktop Update manifests.\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  }
}
