import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { validateProjectVersion } from "./release-version.mjs";

const CHANGELOG_ENTRY_HEADING = /^## (\S+)(?: -[^\n]*)?$/;

/** Validates release notes before Version Bump can create an irreversible tag. */
export function assertReleaseNotes({
  candidate,
  notes,
  changelog,
}) {
  validateProjectVersion(candidate, "Release version");
  const normalizedNotes = normalizeNotes(notes);
  if (!normalizedNotes) {
    throw new Error("Release notes must contain user-facing Markdown");
  }
  if (!/^##\s+\S+/m.test(normalizedNotes) || !/^\s*[-*]\s+\S+/m.test(normalizedNotes)) {
    throw new Error("Release notes must contain Markdown sections and bullet-point user impact");
  }

  for (const entry of changelogEntries(changelog)) {
    if (entry.body === normalizedNotes) {
      throw new Error(`Release notes duplicate the existing ${entry.version} changelog entry`);
    }
  }
  return normalizedNotes;
}

function normalizeNotes(notes) {
  return notes.replace(/\r\n?/g, "\n").trim();
}

function changelogEntries(changelog) {
  return changelog
    .replace(/\r\n?/g, "\n")
    .split(/(?=^## \S)/m)
    .map((section) => {
      const [heading, ...bodyLines] = section.trim().split("\n");
      const match = CHANGELOG_ENTRY_HEADING.exec(heading ?? "");
      if (!match || !/^\d+\.\d+\.\d+(?:-(?:alpha|beta|rc)\.\d+)?$/.test(match[1])) {
        return undefined;
      }
      return {
        version: match[1],
        // Changelog headings are one level deeper than release-note headings.
        body: bodyLines.join("\n").replace(/^#{3,6}(?=\s)/gm, (headingText) => headingText.slice(1)).trim(),
      };
    })
    .filter(Boolean);
}

function main() {
  const [candidate, notesPath] = process.argv.slice(2);
  if (!candidate || !notesPath) {
    throw new Error(
      "Usage: node scripts/validate-release-notes.mjs <version> <notes-path>",
    );
  }
  const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const changelog = readFileSync(path.join(repoRoot, "apps", "vscode-extension", "CHANGELOG.md"), "utf8");
  assertReleaseNotes({
    candidate,
    notes: readFileSync(notesPath, "utf8"),
    changelog,
  });
  console.log(`Validated release notes for ${candidate}.`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
