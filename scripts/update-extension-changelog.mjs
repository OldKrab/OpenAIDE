import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { validateProjectVersion } from "./release-version.mjs";

const CHANGELOG_HEADING = "# Changelog";

/** Builds one Marketplace changelog entry from the same Markdown used by the GitHub release. */
export function formatExtensionChangelogEntry({ version, releaseDate, releaseNotes }) {
  validateProjectVersion(version, "Changelog version");
  if (!/^\d{4}-\d{2}-\d{2}$/.test(releaseDate)) {
    throw new Error(`Release date must use YYYY-MM-DD: ${releaseDate}`);
  }
  const normalizedNotes = releaseNotes.replace(/\r\n?/g, "\n").trim();
  if (!normalizedNotes) {
    throw new Error("Release notes must not be empty");
  }

  // The version owns level two, so release-note sections become its children.
  const nestedNotes = normalizedNotes.replace(/^(#{1,5})(?=\s)/gm, "$1#");
  return `## ${version} - ${releaseDate}\n\n${nestedNotes}`;
}

/** Prepends an entry while preserving the existing user-visible release history. */
export function prependExtensionChangelogEntry({ changelog, entry, version }) {
  const normalized = changelog.replace(/\r\n?/g, "\n").trimEnd();
  if (!normalized.startsWith(`${CHANGELOG_HEADING}\n`)) {
    throw new Error(`Extension changelog must start with ${CHANGELOG_HEADING}`);
  }
  const escapedVersion = version.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  if (new RegExp(`^## ${escapedVersion}(?: -|$)`, "m").test(normalized)) {
    throw new Error(`Extension changelog already contains ${version}`);
  }
  const history = normalized.slice(CHANGELOG_HEADING.length).trimStart();
  return `${CHANGELOG_HEADING}\n\n${entry}\n\n${history}\n`;
}

async function main() {
  const [version, notesPath, changelogArgument] = process.argv.slice(2);
  if (!version || !notesPath) {
    throw new Error(
      "Usage: node scripts/update-extension-changelog.mjs <version> <release-notes-path> [changelog-path]",
    );
  }
  const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const changelogPath = changelogArgument
    ? path.resolve(changelogArgument)
    : path.join(repoRoot, "apps", "vscode-extension", "CHANGELOG.md");
  const releaseDate = process.env.RELEASE_DATE ?? new Date().toISOString().slice(0, 10);
  const [releaseNotes, changelog] = await Promise.all([
    readFile(path.resolve(notesPath), "utf8"),
    readFile(changelogPath, "utf8"),
  ]);
  const entry = formatExtensionChangelogEntry({ version, releaseDate, releaseNotes });
  const updated = prependExtensionChangelogEntry({ changelog, entry, version });
  await writeFile(changelogPath, updated);
  console.log(`Added ${version} to ${path.relative(repoRoot, changelogPath)}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
