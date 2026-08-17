import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import semver from "semver";

const RELEASE_PATTERN = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-(alpha|beta|rc)\.([1-9][0-9]*))?$/;

/** Validates the deliberately narrow, monotonically increasing OpenAIDE release sequence. */
export function validateNextReleaseVersion({ candidate, current, tags = [] }) {
  validateProjectVersion(candidate, "Release version");
  validateProjectVersion(current, "Current package version");

  const releasedVersions = tags
    .map((tag) => tag.startsWith("v") ? tag.slice(1) : tag)
    .filter((version) => RELEASE_PATTERN.test(version));
  const floor = [current, ...releasedVersions].sort(semver.rcompare)[0];
  if (!semver.gt(candidate, floor)) {
    throw new Error(`Release version ${candidate} must be greater than ${floor}`);
  }
  return candidate;
}

/** Selects the changelog baseline without exposing prereleases in stable release notes. */
export function selectPreviousReleaseTag({ candidate, tags = [], canonicalTags }) {
  validateProjectVersion(candidate, "Release version");
  const stableRelease = !semver.prerelease(candidate);
  const baselineTags = canonicalTags ?? tags;
  return baselineTags
    .filter((tag) => tag.startsWith("v"))
    .filter((tag) => RELEASE_PATTERN.test(tag.slice(1)))
    .filter((tag) => semver.lt(tag.slice(1), candidate))
    .filter((tag) => !stableRelease || !semver.prerelease(tag.slice(1)))
    .sort((left, right) => semver.rcompare(left.slice(1), right.slice(1)))[0] ?? "";
}

/** Validates that a release tag and the canonical package version describe one release. */
export function validateReleaseTag({ tag, packageVersion }) {
  if (!tag.startsWith("v")) {
    throw new Error(`Release tag must start with v: ${tag}`);
  }
  const version = tag.slice(1);
  validateProjectVersion(version, "Release tag version");
  if (version !== packageVersion) {
    throw new Error(`Tag version ${version} does not match package.json version ${packageVersion}`);
  }
  return version;
}

export function validateProjectVersion(version, label = "Version") {
  if (!RELEASE_PATTERN.test(version) || semver.valid(version) !== version) {
    throw new Error(
      `${label} must be X.Y.Z or X.Y.Z-(alpha|beta|rc).N with N starting at 1: ${version}`,
    );
  }
  return version;
}

function repositoryFacts(repoRoot) {
  const packageVersion = JSON.parse(readFileSync(path.join(repoRoot, "package.json"), "utf8")).version;
  const tags = execFileSync("git", ["tag", "--list", "v*"], { cwd: repoRoot, encoding: "utf8" })
    .split(/\r?\n/)
    .filter(Boolean);
  return { packageVersion, tags };
}

function main() {
  const [mode, value] = process.argv.slice(2);
  const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const { packageVersion, tags } = repositoryFacts(repoRoot);
  if (mode === "next" && value) {
    console.log(validateNextReleaseVersion({ candidate: value, current: packageVersion, tags }));
    return;
  }
  if (mode === "tag" && value) {
    console.log(validateReleaseTag({ tag: value, packageVersion }));
    return;
  }
  if (mode === "previous-tag" && value) {
    const canonicalTags = process.argv.slice(3);
    console.log(selectPreviousReleaseTag({
      candidate: value,
      tags,
      canonicalTags: canonicalTags.length ? canonicalTags : undefined,
    }));
    return;
  }
  throw new Error("Usage: node scripts/release-version.mjs <next VERSION|tag TAG|previous-tag VERSION [CANONICAL_TAG...]>");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
